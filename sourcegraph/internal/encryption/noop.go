package encryption

import "context"

var _ Key = &NoopKey{}

type NoopKey struct{}

func (k *NoopKey) Encrypt(ctx context.Context, plaintext []byte) ([]byte, error) {
	return plaintext, nil
}

func (k *NoopKey) Decrypt(ctx context.Context, ciphertext []byte) (*Secret, error) {
	s := NewSecret(string(ciphertext))
	return &s, nil
}
