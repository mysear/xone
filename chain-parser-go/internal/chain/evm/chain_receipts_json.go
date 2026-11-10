package evm

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"strings"
)

// encoding/json otherwise accepts the last duplicate (including case-folded
// aliases). Inspect nested log objects too, before decoding any Receipt fields.
func rejectDuplicateReceiptKeys(raw []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	if err := receiptJSONValue(decoder, 0); err != nil {
		return err
	}
	if _, err := decoder.Token(); err != io.EOF {
		return errors.New("RECEIPT_JSON_INVALID")
	}
	return nil
}
func receiptJSONValue(decoder *json.Decoder, depth int) error {
	if depth > 64 {
		return errors.New("RECEIPT_JSON_DEPTH")
	}
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delim, ok := token.(json.Delim)
	if !ok {
		return nil
	}
	switch delim {
	case '{':
		seen := map[string]bool{}
		for decoder.More() {
			token, err := decoder.Token()
			if err != nil {
				return err
			}
			key, ok := token.(string)
			if !ok {
				return errors.New("RECEIPT_JSON_INVALID")
			}
			key = strings.ToLower(key)
			if seen[key] {
				return errors.New("RECEIPT_JSON_DUPLICATE_FIELD")
			}
			seen[key] = true
			if err = receiptJSONValue(decoder, depth+1); err != nil {
				return err
			}
		}
	case '[':
		for decoder.More() {
			if err = receiptJSONValue(decoder, depth+1); err != nil {
				return err
			}
		}
	default:
		return errors.New("RECEIPT_JSON_INVALID")
	}
	_, err = decoder.Token()
	return err
}
