package domain

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
)

// V3 core is an older shared type. Check every object before its ordinary
// decoder can erase duplicate keys, including escaped spellings of a key.
func pancakeStrictJSON(raw []byte, out any, required []string) error {
	d := json.NewDecoder(bytes.NewReader(raw))
	d.UseNumber()
	var value func(int) error
	value = func(depth int) error {
		if depth > 128 {
			return errors.New("PANCAKE_LM_JSON_DEPTH")
		}
		token, e := d.Token()
		if e != nil {
			return e
		}
		delimiter, ok := token.(json.Delim)
		if !ok {
			return nil
		}
		switch delimiter {
		case '{':
			seen := map[string]bool{}
			for d.More() {
				token, e := d.Token()
				if e != nil {
					return e
				}
				key, ok := token.(string)
				if !ok || seen[key] {
					return errors.New("PANCAKE_LM_JSON_DUPLICATE")
				}
				seen[key] = true
				if e = value(depth + 1); e != nil {
					return e
				}
			}
			close, e := d.Token()
			if e != nil || close != json.Delim('}') {
				return errors.New("PANCAKE_LM_JSON_OBJECT")
			}
		case '[':
			for d.More() {
				if e := value(depth + 1); e != nil {
					return e
				}
			}
			close, e := d.Token()
			if e != nil || close != json.Delim(']') {
				return errors.New("PANCAKE_LM_JSON_ARRAY")
			}
		default:
			return errors.New("PANCAKE_LM_JSON_DELIMITER")
		}
		return nil
	}
	if e := value(0); e != nil {
		return e
	}
	if _, e := d.Token(); e != io.EOF {
		return errors.New("PANCAKE_LM_JSON_TRAILING")
	}
	return lfjStrictJSON(raw, out, required)
}
