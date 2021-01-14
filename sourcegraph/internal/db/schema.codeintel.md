# Table "public.codeintel_schema_migrations"
```
 Column  |  Type   | Modifiers 
---------+---------+-----------
 version | bigint  | not null
 dirty   | boolean | not null
Indexes:
    "codeintel_schema_migrations_pkey" PRIMARY KEY, btree (version)

```

Holds a single column storing the status of the most recent migration attempt.

**dirty**: Whether or not the most recent migration attempt failed.

**version**: The schema version that was the target of the most recent migration attempt.

# Table "public.lsif_data_definitions"
```
   Column   |  Type   | Modifiers 
------------+---------+-----------
 dump_id    | integer | not null
 scheme     | text    | not null
 identifier | text    | not null
 data       | bytea   | 
Indexes:
    "lsif_data_definitions_pkey" PRIMARY KEY, btree (dump_id, scheme, identifier)

```

Associates (document, range) pairs with the import monikers attached to the range.

**data**: A gob-encoded payload conforming to an array of [LocationData](https://sourcegraph.com/github.com/sourcegraph/sourcegraph@3.23/-/blob/enterprise/internal/codeintel/stores/lsifstore/types.go#L100:6) types.

**dump_id**: The identifier of the associated dump in the lsif_uploads table (state=completed).

**identifier**: The moniker identifier.

**scheme**: The moniker scheme.

# Table "public.lsif_data_documents"
```
 Column  |  Type   | Modifiers 
---------+---------+-----------
 dump_id | integer | not null
 path    | text    | not null
 data    | bytea   | 
Indexes:
    "lsif_data_documents_pkey" PRIMARY KEY, btree (dump_id, path)

```

Stores reference, hover text, moniker, and diagnostic data about a particular text document witin a dump.

**data**: A gob-encoded payload conforming to the [DocumentData](https://sourcegraph.com/github.com/sourcegraph/sourcegraph@3.23/-/blob/enterprise/internal/codeintel/stores/lsifstore/types.go#L13:6) type.

**dump_id**: The identifier of the associated dump in the lsif_uploads table (state=completed).

**path**: The path of the text document relative to the associated dump root.

# Table "public.lsif_data_metadata"
```
      Column       |  Type   | Modifiers 
-------------------+---------+-----------
 dump_id           | integer | not null
 num_result_chunks | integer | 
Indexes:
    "lsif_data_metadata_pkey" PRIMARY KEY, btree (dump_id)

```

Stores the number of result chunks associated with a dump.

**dump_id**: The identifier of the associated dump in the lsif_uploads table (state=completed).

**num_result_chunks**: A bound of populated indexes in the lsif_data_result_chunks table for the associated dump. This value is used to hash identifiers into the result chunk index to which they belong.

# Table "public.lsif_data_references"
```
   Column   |  Type   | Modifiers 
------------+---------+-----------
 dump_id    | integer | not null
 scheme     | text    | not null
 identifier | text    | not null
 data       | bytea   | 
Indexes:
    "lsif_data_references_pkey" PRIMARY KEY, btree (dump_id, scheme, identifier)

```

Associates (document, range) pairs with the export monikers attached to the range.

**data**: A gob-encoded payload conforming to an array of [LocationData](https://sourcegraph.com/github.com/sourcegraph/sourcegraph@3.23/-/blob/enterprise/internal/codeintel/stores/lsifstore/types.go#L100:6) types.

**dump_id**: The identifier of the associated dump in the lsif_uploads table (state=completed).

**identifier**: The moniker identifier.

**scheme**: The moniker scheme.

# Table "public.lsif_data_result_chunks"
```
 Column  |  Type   | Modifiers 
---------+---------+-----------
 dump_id | integer | not null
 idx     | integer | not null
 data    | bytea   | 
Indexes:
    "lsif_data_result_chunks_pkey" PRIMARY KEY, btree (dump_id, idx)

```

Associates result set identifiers with the (document path, range identifier) pairs that compose the set.

**data**: A gob-encoded payload conforming to the [ResultChunkData](https://sourcegraph.com/github.com/sourcegraph/sourcegraph@3.23/-/blob/enterprise/internal/codeintel/stores/lsifstore/types.go#L70:6) type.

**dump_id**: The identifier of the associated dump in the lsif_uploads table (state=completed).

**idx**: The unique result chunk index within the associated dump. Every result set identifier present should hash to this index (modulo lsif_data_metadata.num_result_chunks).
