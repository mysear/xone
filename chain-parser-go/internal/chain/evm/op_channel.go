package evm

import (
	"bytes"
	"compress/zlib"
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"

	"github.com/andybalholm/brotli"
	"github.com/ethereum/go-ethereum/crypto/kzg4844"
	"github.com/ethereum/go-ethereum/rlp"
)

// Four field elements carry 124 literal bytes and three bytes split across their high sextets.
func unpackOPBlob(blob *kzg4844.Blob) ([]byte, error) {
	if blob == nil || blob[1] != 0 {
		return nil, errors.New("OP_BLOB_VERSION")
	}
	const size = 1024 * 127
	n := int(blob[2])<<16 | int(blob[3])<<8 | int(blob[4])
	if n > size-4 {
		return nil, errors.New("OP_BLOB_LENGTH")
	}
	packed := make([]byte, 0, size)
	for offset := 0; offset < len(blob); offset += 128 {
		a, b, c, d := blob[offset], blob[offset+32], blob[offset+64], blob[offset+96]
		if (a|b|c|d)&0xc0 != 0 {
			return nil, errors.New("OP_BLOB_FIELD_ENCODING")
		}
		packed = append(packed, blob[offset+1:offset+32]...)
		packed = append(packed, a|(b&48)<<2)
		packed = append(packed, blob[offset+33:offset+64]...)
		packed = append(packed, b&15|(d&15)<<4)
		packed = append(packed, blob[offset+65:offset+96]...)
		packed = append(packed, c|(d&48)<<2)
		packed = append(packed, blob[offset+97:offset+128]...)
	}
	if len(bytes.Trim(packed[4+n:], "\x00")) != 0 {
		return nil, errors.New("OP_BLOB_PADDING")
	}
	return packed[4 : 4+n], nil
}

type OPChannelReport struct {
	Span            *OPSpanReport `json:"span,omitempty"`
	ID              string        `json:"channel_id"`
	Frames          int           `json:"frames"`
	Compression     string        `json:"compression"`
	CompressedBytes int           `json:"compressed_bytes"`
	DecodedBytes    int           `json:"decoded_bytes"`
	DecodedSHA256   string        `json:"decoded_sha256"`
	SingularBatches int           `json:"singular_batches"`
	SpanBatches     int           `json:"span_batches"`
}

type opFrames struct {
	id         [16]byte
	count      int
	compressed []byte
	closed     bool
}

func (c *opFrames) appendPayload(payload []byte) error {
	if len(payload) < 2 || payload[0] != 0 {
		return errors.New("OP_FRAME_VERSION")
	}
	for data := payload[1:]; len(data) > 0; {
		if len(data) < 23 {
			return errors.New("OP_FRAME_TRUNCATED")
		}
		size := binary.BigEndian.Uint32(data[18:22])
		if size > 1_000_000 || uint64(size)+23 > uint64(len(data)) {
			return errors.New("OP_FRAME_LENGTH")
		}
		id := [16]byte(data[:16])
		seq := int(binary.BigEndian.Uint16(data[16:18]))
		end := data[22+int(size)]
		if c.closed || seq != c.count || (c.count > 0 && id != c.id) || end > 1 {
			return errors.New("OP_FRAME_ORDER")
		}
		if len(c.compressed)+int(size) > 16<<20 {
			return errors.New("OP_CHANNEL_SIZE")
		}
		c.id = id
		c.count++
		c.closed = end == 1
		c.compressed = append(c.compressed, data[22:22+int(size)]...)
		data = data[23+int(size):]
	}
	return nil
}

func (frames *opFrames) appendBlobs(ctx context.Context, blobs []*kzg4844.Blob) error {
	for _, blob := range blobs {
		if err := ctx.Err(); err != nil {
			return err
		}
		data, err := unpackOPBlob(blob)
		if err != nil {
			return err
		}
		if err = frames.appendPayload(data); err != nil {
			return err
		}
	}
	return nil
}

func decodeOPChannel(ctx context.Context, blobs []*kzg4844.Blob, blocks []json.RawMessage) (*OPChannelReport, error) {
	var frames opFrames
	if err := frames.appendBlobs(ctx, blobs); err != nil {
		return nil, err
	}
	return frames.decode(ctx, blocks)
}

func (frames *opFrames) decode(ctx context.Context, blocks []json.RawMessage) (*OPChannelReport, error) {
	return frames.decodeWithLoader(ctx, blocks, nil)
}

func (frames *opFrames) decodeWithLoader(ctx context.Context, blocks []json.RawMessage, load opSpanLoader) (*OPChannelReport, error) {
	if !frames.closed || len(frames.compressed) == 0 {
		return nil, errors.New("OP_CHANNEL_INCOMPLETE")
	}
	report := &OPChannelReport{ID: hex.EncodeToString(frames.id[:]), Frames: frames.count, CompressedBytes: len(frames.compressed)}
	source := bytes.NewReader(frames.compressed)
	var decoder io.Reader
	if frames.compressed[0] == 1 {
		source.ReadByte()
		report.Compression = "brotli"
		decoder = brotli.NewReader(source)
	} else {
		reader, err := zlib.NewReader(source)
		if err != nil {
			return nil, errors.New("OP_CHANNEL_COMPRESSION")
		}
		defer reader.Close()
		report.Compression = "zlib"
		decoder = reader
	}
	const maxDecoded = 32 << 20
	raw, err := io.ReadAll(io.LimitReader(decoder, maxDecoded+1))
	if err != nil || source.Len() != 0 || len(raw) == 0 || len(raw) > maxDecoded {
		return nil, errors.New("OP_CHANNEL_DECOMPRESSION")
	}
	report.DecodedBytes = len(raw)
	report.DecodedSHA256 = fmt.Sprintf("%x", sha256.Sum256(raw))
	for len(raw) > 0 {
		if err = ctx.Err(); err != nil {
			return nil, err
		}
		batch, rest, err := rlp.SplitString(raw)
		if err != nil || len(batch) < 2 {
			return nil, errors.New("OP_CHANNEL_BATCH_RLP")
		}
		if blocks != nil || load != nil {
			if batch[0] != 1 || len(rest) != 0 {
				return nil, errors.New("OP_CHANNEL_SINGLE_SPAN_REQUIRED")
			}
			if load != nil {
				first, count, err := opSpanHeightRange(batch)
				if err != nil {
					return nil, err
				}
				blocks, err = load(ctx, first, count)
				if err != nil {
					return nil, err
				}
			}
			report.Span, err = verifyOPSpan(ctx, batch, blocks)
			if err != nil {
				return nil, err
			}
		}
		switch batch[0] {
		case 0:
			report.SingularBatches++
		case 1:
			report.SpanBatches++
		default:
			return nil, errors.New("OP_CHANNEL_BATCH_TYPE")
		}
		raw = rest
	}
	return report, nil
}
