package journal

import (
	"context"
	"encoding/hex"
	"errors"
	"github.com/twmb/franz-go/pkg/kgo"
	"github.com/twmb/franz-go/pkg/kmsg"
)

// Reader has no publication or producer-fencing interface. It reads complete
// units from an exact committed cursor, never skips retention gaps or chunks.
type Reader struct{ k *Kafka }

func OpenReadOnly(ctx context.Context, brokers []string, topic string, maxBytes, maxRecords int) (*Reader, error) {
	if topic == "" || len(brokers) == 0 || maxBytes < 1024 || maxBytes > 64<<20 || maxRecords < 3 || maxRecords > 100000 {
		return nil, errors.New("ANALYTICS_READER_LIMIT")
	}
	client, e := kgo.NewClient(kgo.SeedBrokers(brokers...))
	if e != nil {
		return nil, errors.New("KAFKA_CLIENT_CONFIG")
	}
	k := &Kafka{client: client, Topic: topic, maxBytes: maxBytes, maxTail: maxRecords}
	if _, e = k.leader(ctx); e != nil {
		client.Close()
		return nil, e
	}
	return &Reader{k: k}, nil
}
func (r *Reader) Close()          { r.k.Close() }
func (r *Reader) TopicID() string { return hex.EncodeToString(r.k.topicID[:]) }
func (r *Reader) Consume(ctx context.Context, cursor Cursor, apply func(Unit, Cursor) error) error {
	k := r.k
	if cursor.Topic != k.Topic || cursor.Partition != 0 || cursor.Offset < 0 {
		return errors.New("ANALYTICS_CURSOR_IDENTITY")
	}
	leader, e := k.leader(ctx)
	if e != nil {
		return e
	}
	low, e := k.offset(ctx, leader, -2)
	if e != nil {
		return e
	}
	end, e := k.offset(ctx, leader, -1)
	if e != nil {
		return e
	}
	if cursor.Offset < low {
		return errors.New("ANALYTICS_RETENTION_GAP")
	}
	if cursor.Offset > end || end-cursor.Offset > int64(k.maxTail) {
		return errors.New("ANALYTICS_TAIL_LIMIT")
	}
	at := cursor.Offset
	a := NewAssembler(k.maxBytes)
	for at < end {
		if ctx.Err() != nil {
			return errors.New("ANALYTICS_READ_DEADLINE")
		}
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
			return errors.New("ANALYTICS_FETCH")
		}
		response, ok := raw.(*kmsg.FetchResponse)
		if !ok || len(response.Topics) != 1 || len(response.Topics[0].Partitions) != 1 {
			return errors.New("ANALYTICS_FETCH_RESPONSE")
		}
		fp, next := kgo.ProcessFetchPartition(kgo.ProcessFetchPartitionOpts{Topic: k.Topic, Partition: 0, Offset: at, IsolationLevel: kgo.ReadCommitted()}, &response.Topics[0].Partitions[0], kgo.DefaultDecompressor(), nil)
		if fp.Err != nil {
			return errors.New("ANALYTICS_FETCH_PARTITION")
		}
		for _, record := range fp.Records {
			if record.Offset >= end {
				break
			}
			f, e := DecodeFrame(record.Value)
			if e != nil {
				return e
			}
			unit, e := a.Push(f)
			if e != nil {
				return e
			}
			if unit != nil {
				if e = apply(*unit, Cursor{Topic: k.Topic, Partition: 0, Offset: record.Offset + 1}); e != nil {
					return e
				}
			}
		}
		if next <= at {
			return errors.New("ANALYTICS_FETCH_NO_PROGRESS")
		}
		at = next
	}
	if a.begin != nil {
		return errors.New("ANALYTICS_INCOMPLETE_UNIT")
	}
	return nil
}
