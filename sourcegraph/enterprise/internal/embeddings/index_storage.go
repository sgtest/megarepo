package embeddings

import (
	"bytes"
	"context"
	"encoding/gob"
	"io"

	"golang.org/x/sync/errgroup"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/types"
	"github.com/sourcegraph/sourcegraph/internal/uploadstore"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// IndexFormatVersion is a number representing the on-disk index format. Whenever the index format is changed in a
// way that affects how it's decoded, we add a new format version and update CurrentFormatVersion to the latest.
type IndexFormatVersion int

const CurrentFormatVersion = EmbeddingModelVersion
const (
	InitialVersion        IndexFormatVersion = iota // The initial format, before we started tracking format versions
	EmbeddingModelVersion                           // Added the model name used to create embeddings
)

func DownloadIndex[T any](ctx context.Context, uploadStore uploadstore.Store, key string) (_ *T, err error) {
	file, err := uploadStore.Get(ctx, key)
	if err != nil {
		return nil, err
	}
	defer func() { err = errors.Append(err, file.Close()) }()

	var index T
	if err = gob.NewDecoder(file).Decode(&index); err != nil {
		return nil, err
	}
	return &index, nil
}

func UploadIndex[T any](ctx context.Context, uploadStore uploadstore.Store, key string, index T) error {
	buffer := bytes.NewBuffer(nil)
	if err := gob.NewEncoder(buffer).Encode(index); err != nil {
		return err
	}

	_, err := uploadStore.Upload(ctx, key, buffer)
	return err
}

func UploadRepoEmbeddingIndex(ctx context.Context, uploadStore uploadstore.Store, key string, index *RepoEmbeddingIndex) error {
	pr, pw := io.Pipe()

	eg, ctx := errgroup.WithContext(ctx)

	eg.Go(func() error {
		defer pw.Close()

		enc := newEncoder(gob.NewEncoder(pw), CurrentFormatVersion, embeddingsChunkSize)
		return enc.encode(index)
	})

	eg.Go(func() error {
		defer pr.Close()

		_, err := uploadStore.Upload(ctx, key, pr)
		return err
	})

	return eg.Wait()
}

func UpdateRepoEmbeddingIndex(
	ctx context.Context,
	uploadStore uploadstore.Store,
	key string,
	previous *RepoEmbeddingIndex,
	new *RepoEmbeddingIndex,
	toRemove []string,
	ranks types.RepoPathRanks,
) error {
	// update revision
	previous.Revision = new.Revision
	// set the model (older indexes didn't include the model)
	previous.EmbeddingsModel = new.EmbeddingsModel

	// filter based on toRemove
	toRemoveSet := make(map[string]struct{}, len(toRemove))
	for _, s := range toRemove {
		toRemoveSet[s] = struct{}{}
	}
	previous.CodeIndex.filter(toRemoveSet, ranks)
	previous.TextIndex.filter(toRemoveSet, ranks)

	// append new data
	previous.CodeIndex.append(new.CodeIndex)
	previous.TextIndex.append(new.TextIndex)

	// re-upload
	return UploadRepoEmbeddingIndex(ctx, uploadStore, key, previous)
}

func DownloadRepoEmbeddingIndex(ctx context.Context, uploadStore uploadstore.Store, key string) (*RepoEmbeddingIndex, error) {
	dec, err := newDecoder(ctx, uploadStore, key)
	if err != nil {
		return nil, err
	}
	defer dec.close()

	rei, err := dec.decode()

	// If decoding fails, assume it is an old index and decode with a generic dec.
	if err != nil {
		oldRei, err2 := DownloadIndex[OldRepoEmbeddingIndex](ctx, uploadStore, key)
		if err2 != nil {
			return nil, errors.Append(err, err2)
		}
		return oldRei.ToNewIndex(), nil
	}

	return rei, nil
}

type decoder struct {
	file          io.ReadCloser
	dec           *gob.Decoder
	formatVersion IndexFormatVersion
}

func newDecoder(ctx context.Context, uploadStore uploadstore.Store, key string) (*decoder, error) {
	f, err := uploadStore.Get(ctx, key)
	if err != nil {
		return nil, err
	}

	dec := gob.NewDecoder(f)
	var formatVersion IndexFormatVersion
	if err := dec.Decode(&formatVersion); err != nil {
		// If there's an error, assume this is an old index that doesn't encode the
		// version. Open the file again to reset the reader.
		if err := f.Close(); err != nil {
			return nil, err
		}

		f, err = uploadStore.Get(ctx, key)
		if err != nil {
			return nil, err
		}
		dec = gob.NewDecoder(f)
		return &decoder{f, dec, InitialVersion}, nil
	}

	if formatVersion > CurrentFormatVersion {
		return nil, errors.Newf("unrecognized index format version: %d", formatVersion)
	}
	return &decoder{f, dec, formatVersion}, nil
}

func (d *decoder) decode() (*RepoEmbeddingIndex, error) {
	rei := &RepoEmbeddingIndex{}

	if err := d.dec.Decode(&rei.RepoName); err != nil {
		return nil, err
	}

	if err := d.dec.Decode(&rei.Revision); err != nil {
		return nil, err
	}

	if d.formatVersion >= EmbeddingModelVersion {
		if err := d.dec.Decode(&rei.EmbeddingsModel); err != nil {
			return nil, err
		}
	}

	for _, ei := range []*EmbeddingIndex{&rei.CodeIndex, &rei.TextIndex} {
		if err := d.dec.Decode(&ei.ColumnDimension); err != nil {
			return nil, err
		}

		if err := d.dec.Decode(&ei.RowMetadata); err != nil {
			return nil, err
		}

		if err := d.dec.Decode(&ei.Ranks); err != nil {
			return nil, err
		}

		var numChunks int
		if err := d.dec.Decode(&numChunks); err != nil {
			return nil, err
		}

		ei.Embeddings = make([]int8, 0, numChunks*ei.ColumnDimension)
		for i := 0; i < numChunks; i++ {
			var embeddingSlice []float32
			if err := d.dec.Decode(&embeddingSlice); err != nil {
				return nil, err
			}
			ei.Embeddings = append(ei.Embeddings, Quantize(embeddingSlice)...)
		}
	}

	return rei, nil
}

func (d *decoder) close() {
	d.file.Close()
}

const embeddingsChunkSize = 10_000

// encoder is a specialized encoder for repo embedding indexes. Instead of GOB-encoding
// the entire RepoEmbeddingIndex, we encode each field separately, and we encode the embeddings array by chunks.
// This way we avoid allocating a separate very large slice for the embeddings.
type encoder struct {
	enc *gob.Encoder
	// In production usage, formatVersion will always be equal to CurrentFormatVersion. But it's still
	// a parameter here since it's helpful for unit tests to be able to change it.
	formatVersion IndexFormatVersion
	chunkSize     int
}

func newEncoder(enc *gob.Encoder, formatVersion IndexFormatVersion, chunkSize int) *encoder {
	return &encoder{enc, formatVersion, chunkSize}
}

func (e *encoder) encode(rei *RepoEmbeddingIndex) error {
	// Always encode index format version first, as part of 'file header'
	if err := e.enc.Encode(e.formatVersion); err != nil {
		return err
	}

	if err := e.enc.Encode(rei.RepoName); err != nil {
		return err
	}

	if err := e.enc.Encode(rei.Revision); err != nil {
		return err
	}

	if e.formatVersion >= EmbeddingModelVersion {
		if err := e.enc.Encode(rei.EmbeddingsModel); err != nil {
			return err
		}
	}

	for _, ei := range []EmbeddingIndex{rei.CodeIndex, rei.TextIndex} {
		if err := e.enc.Encode(ei.ColumnDimension); err != nil {
			return err
		}

		if err := e.enc.Encode(ei.RowMetadata); err != nil {
			return err
		}

		if err := e.enc.Encode(ei.Ranks); err != nil {
			return err
		}

		numChunks := (len(ei.Embeddings) + e.chunkSize - 1) / e.chunkSize
		if err := e.enc.Encode(numChunks); err != nil {
			return err
		}

		for i := 0; i < numChunks; i++ {
			start := i * e.chunkSize
			end := start + e.chunkSize

			if end > len(ei.Embeddings) {
				end = len(ei.Embeddings)
			}

			if err := e.enc.Encode(Dequantize(ei.Embeddings[start:end])); err != nil {
				return err
			}
		}
	}

	return nil
}
