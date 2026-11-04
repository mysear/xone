package journal

import (
	"context"

	"errors"
	"github.com/twmb/franz-go/pkg/kerr"
	"github.com/twmb/franz-go/pkg/kgo"
	"github.com/twmb/franz-go/pkg/kmsg"
	"time"
)

type Kafka struct {
	client     *kgo.Client
	brokers    []string
	Topic      string
	chunkBytes int
	maxBytes   int
	maxTail    int
	topicID    [16]byte
	sealed     bool
}

func OpenKafka(ctx context.Context, brokers []string, topic, transactionID string, chunkBytes, maxBytes, maxTail int, timeout time.Duration) (*Kafka, error) {
	opts := []kgo.Opt{kgo.SeedBrokers(brokers...), kgo.RequiredAcks(kgo.AllISRAcks()), kgo.RecordPartitioner(kgo.ManualPartitioner()), kgo.ProducerBatchCompression(kgo.NoCompression()), kgo.ProducerBatchMaxBytes(int32(chunkBytes * 3))}
	if transactionID != "" {
		opts = append(opts, kgo.TransactionalID(transactionID), kgo.TransactionTimeout(timeout))
	}
	client, e := kgo.NewClient(opts...)
	if e != nil {
		return nil, errors.New("KAFKA_CLIENT_CONFIG")
	}
	k := &Kafka{client: client, brokers: brokers, Topic: topic, chunkBytes: chunkBytes, maxBytes: maxBytes, maxTail: maxTail}
	if _, e = k.leader(ctx); e != nil {
		client.Close()
		return nil, e
	}
	if transactionID != "" {
		initCtx, cancel := context.WithTimeout(ctx, timeout)
		defer cancel()
		for {
			_, _, e = client.ProducerID(initCtx)
			if e == nil {
				break
			}
			if !errors.Is(e, kerr.ConcurrentTransactions) && !kerr.IsRetriable(e) {
				client.Close()
				return nil, errors.New("KAFKA_INIT_TRANSACTIONS_FATAL")
			}
			timer := time.NewTimer(100 * time.Millisecond)
			select {
			case <-initCtx.Done():
				timer.Stop()
				client.Close()
				return nil, errors.New("KAFKA_INIT_TRANSACTIONS_TIMEOUT")
			case <-timer.C:
			}
		}
	}

	return k, nil
}
func (k *Kafka) Close() { k.client.Close() }
func (k *Kafka) leader(ctx context.Context) (int32, error) {
	q := kmsg.NewPtrMetadataRequest()
	q.AllowAutoTopicCreation = false
	topic := k.Topic
	q.Topics = []kmsg.MetadataRequestTopic{{Topic: &topic}}
	raw, e := k.client.Request(ctx, q)
	if e != nil {
		return 0, errors.New("KAFKA_METADATA")
	}
	r, ok := raw.(*kmsg.MetadataResponse)
	if !ok || len(r.Topics) != 1 || r.Topics[0].ErrorCode != 0 || len(r.Topics[0].Partitions) != 1 || r.Topics[0].Partitions[0].Partition != 0 || r.Topics[0].Partitions[0].ErrorCode != 0 {
		return 0, errors.New("KAFKA_REQUIRES_SINGLE_PARTITION")
	}
	if k.topicID != ([16]byte{}) && k.topicID != r.Topics[0].TopicID {
		return 0, errors.New("KAFKA_TOPIC_ID_CHANGED")
	}
	k.topicID = r.Topics[0].TopicID
	return r.Topics[0].Partitions[0].Leader, nil
}
func (k *Kafka) Publish(ctx context.Context, unit Unit, beforeCommit func(context.Context) error) (Cursor, error) {
	if k.sealed {
		return Cursor{}, errors.New("KAFKA_SESSION_SEALED")
	}
	if beforeCommit != nil {
		if e := beforeCommit(ctx); e != nil {
			return Cursor{}, e
		}
	}
	if _, e := k.leader(ctx); e != nil {
		return Cursor{}, e
	}
	frames, e := Encode(unit, k.chunkBytes)
	if e != nil {
		return Cursor{}, e
	}
	if frames[0].PayloadBytes > uint64(k.maxBytes) {
		return Cursor{}, errors.New("JOURNAL_UNIT_LIMIT")
	}
	if e = k.client.BeginTransaction(); e != nil {
		return Cursor{}, errors.New("KAFKA_BEGIN_TRANSACTION")
	}
	records := make([]*kgo.Record, 0, len(frames))
	for _, frame := range frames {
		b, e := EncodeFrame(frame)
		if e != nil {
			return Cursor{}, errors.New("KAFKA_ENCODE")
		}
		records = append(records, &kgo.Record{Topic: k.Topic, Partition: 0, Key: []byte(frame.Reference.Block.ChainKey), Value: b})
	}
	results := k.client.ProduceSync(ctx, records...)
	if results.FirstErr() != nil {
		k.sealed = true
		k.client.Close()
		return Cursor{}, errors.New("KAFKA_PRODUCE_FAILED")
	}
	if beforeCommit != nil {
		if e = beforeCommit(ctx); e != nil {
			k.sealed = true
			k.client.Close()
			return Cursor{}, e
		}
	}
	if e = k.client.EndTransaction(ctx, kgo.TryCommit); e != nil {
		k.sealed = true
		k.client.Close()
		return Cursor{}, errors.New("KAFKA_COMMIT_OUTCOME_UNKNOWN")
	}
	return Cursor{k.Topic, 0, records[len(records)-1].Offset + 1}, nil
}
func (k *Kafka) offset(ctx context.Context, leader int32, timestamp int64) (int64, error) {
	q := kmsg.NewPtrListOffsetsRequest()
	q.ReplicaID = -1
	q.IsolationLevel = 1
	p := kmsg.NewListOffsetsRequestTopicPartition()
	p.Partition = 0
	p.Timestamp = timestamp
	q.Topics = []kmsg.ListOffsetsRequestTopic{{Topic: k.Topic, Partitions: []kmsg.ListOffsetsRequestTopicPartition{p}}}
	raw, e := k.client.Broker(int(leader)).Request(ctx, q)
	if e != nil {
		return 0, errors.New("KAFKA_OFFSETS")
	}
	r := raw.(*kmsg.ListOffsetsResponse)
	if len(r.Topics) != 1 || len(r.Topics[0].Partitions) != 1 || r.Topics[0].Partitions[0].ErrorCode != 0 {
		return 0, errors.New("KAFKA_OFFSETS")
	}
	return r.Topics[0].Partitions[0].Offset, nil
}

// Tail scans a finite read_committed window after fencing, including aborted/control offsets.
// The exported Kafka parser returns the physical next offset even for filtered control batches.
func (k *Kafka) Tail(ctx context.Context) (*Unit, Cursor, error) {
	leader, e := k.leader(ctx)
	if e != nil {
		return nil, Cursor{}, e
	}
	low, e := k.offset(ctx, leader, -2)
	if e != nil {
		return nil, Cursor{}, e
	}
	end, e := k.offset(ctx, leader, -1)
	if e != nil {
		return nil, Cursor{}, e
	}
	at := max(low, end-int64(k.maxTail))
	assembler := NewAssembler(k.maxBytes)
	var latest *Unit
	cursor := Cursor{Topic: k.Topic, Partition: 0, Offset: end}
	for at < end {
		q := kmsg.NewPtrFetchRequest()
		q.ReplicaID = -1
		q.MaxWaitMillis = 100
		q.MinBytes = 1
		q.MaxBytes = int32(k.maxBytes)
		q.IsolationLevel = 1
		p := kmsg.NewFetchRequestTopicPartition()
		p.Partition = 0
		p.FetchOffset = at
		p.PartitionMaxBytes = int32(k.maxBytes)
		q.Topics = []kmsg.FetchRequestTopic{{Topic: k.Topic, TopicID: k.topicID, Partitions: []kmsg.FetchRequestTopicPartition{p}}}
		raw, e := k.client.Broker(int(leader)).Request(ctx, q)
		if e != nil {
			return nil, Cursor{}, errors.New("KAFKA_TAIL_FETCH")
		}
		r := raw.(*kmsg.FetchResponse)
		if len(r.Topics) != 1 || len(r.Topics[0].Partitions) != 1 {
			return nil, Cursor{}, errors.New("KAFKA_TAIL_RESPONSE")
		}
		fp, next := kgo.ProcessFetchPartition(kgo.ProcessFetchPartitionOpts{Topic: k.Topic, Partition: 0, Offset: at, IsolationLevel: kgo.ReadCommitted()}, &r.Topics[0].Partitions[0], kgo.DefaultDecompressor(), nil)
		if fp.Err != nil {
			return nil, Cursor{}, errors.New("KAFKA_TAIL_PARTITION")
		}
		for _, record := range fp.Records {
			if record.Offset >= end {
				break
			}
			frame, decodeError := DecodeFrame(record.Value)
			if decodeError != nil {
				return nil, Cursor{}, errors.New("KAFKA_FRAME_JSON")
			}
			if assembler.begin == nil && frame.Kind != "BEGIN" {
				continue
			}
			unit, e := assembler.Push(frame)
			if e != nil {
				return nil, Cursor{}, e
			}
			if unit != nil {
				latest = unit
				cursor.Offset = record.Offset + 1
			}
		}
		if next <= at {
			return nil, Cursor{}, errors.New("KAFKA_TAIL_NO_PROGRESS")
		}
		at = next
	}
	if assembler.begin != nil {
		return nil, Cursor{}, errors.New("KAFKA_INCOMPLETE_TRANSACTION_UNIT")
	}
	if latest == nil && end != low {
		return nil, Cursor{}, errors.New("KAFKA_TAIL_BASELINE_NOT_FOUND")
	}
	return latest, cursor, nil
}
