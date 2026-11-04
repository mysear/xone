package domain

import "errors"

// ArrayInitialized uses original extension 512-bit group ordering. In particular,
// -1024 maps to negative group0 bit0, -513 to group0 bit511.
func (s RaydiumCLMMState) ArrayInitialized(index int32) (bool, error) {
	if len(s.Bitmap) != 16 || s.TickSpacing == 0 || s.TickSpacing > 1000 {
		return false, errors.New("RAYCLMM_BITMAP_WIDTH")
	}
	if index >= -512 && index <= 511 {
		bit := index + 512
		return s.Bitmap[bit/64]&(uint64(1)<<uint(bit%64)) != 0, nil
	}
	e := s.BitmapExtension
	if e == nil {
		return false, errors.New("RAYCLMM_BITMAP_EXTENSION_REQUIRED")
	}
	if len(e.Positive) != 112 || len(e.Negative) != 112 {
		return false, errors.New("RAYCLMM_EXTENSION_WIDTH")
	}
	if index < -7680 || index > 7679 {
		return false, errors.New("RAYCLMM_ARRAY_INDEX")
	}
	var words []uint64
	offset := index - 512
	if index >= 512 {
		words = e.Positive
	} else {
		group := (-index-1)/512 - 1
		bit := index + (group+2)*512
		offset = group*512 + bit
		words = e.Negative
	}
	return words[offset/64]&(uint64(1)<<uint(offset%64)) != 0, nil
}
