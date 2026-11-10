package ingest

// FactoryProofReference computes the Archive identity without acquiring raw or
// pin ownership. Catalogs use it to persist a bounded write intent before
// PutPinned; the Archive still validates the actual write and digest.
func FactoryProofReference(owner Owner, proof []byte) (RawRef, error) {
	if !validOwner(owner) {
		return RawRef{}, ErrOwnership
	}
	return RawRef{ID: jsonID(owner), Digest: digest(proof), Owner: owner, Bytes: int64(len(proof))}, nil
}
