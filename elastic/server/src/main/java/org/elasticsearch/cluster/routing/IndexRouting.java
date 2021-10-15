/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.routing;

import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.ParsingException;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.transport.Transports;
import org.elasticsearch.xcontent.DeprecationHandler;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xcontent.XContentParser.Token;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xcontent.support.filtering.FilterPath;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Set;
import java.util.function.IntConsumer;

import static org.elasticsearch.common.xcontent.XContentParserUtils.ensureExpectedToken;

/**
 * Generates the shard id for {@code (id, routing)} pairs.
 */
public abstract class IndexRouting {
    /**
     * Build the routing from {@link IndexMetadata}.
     */
    public static IndexRouting fromIndexMetadata(IndexMetadata indexMetadata) {
        if (false == indexMetadata.getRoutingPaths().isEmpty()) {
            if (indexMetadata.isRoutingPartitionedIndex()) {
                throw new IllegalArgumentException("routing_partition_size is incompatible with routing_path");
            }
            return new ExtractFromSource(
                indexMetadata.getRoutingNumShards(),
                indexMetadata.getRoutingFactor(),
                indexMetadata.getIndex().getName(),
                indexMetadata.getRoutingPaths()
            );
        }
        if (indexMetadata.isRoutingPartitionedIndex()) {
            return new Partitioned(
                indexMetadata.getRoutingNumShards(),
                indexMetadata.getRoutingFactor(),
                indexMetadata.getRoutingPartitionSize()
            );
        }
        return new Unpartitioned(indexMetadata.getRoutingNumShards(), indexMetadata.getRoutingFactor());
    }

    private final int routingNumShards;
    private final int routingFactor;

    private IndexRouting(int routingNumShards, int routingFactor) {
        this.routingNumShards = routingNumShards;
        this.routingFactor = routingFactor;
    }

    /**
     * Called when indexing a document to generate the shard id that should contain
     * a document with the provided parameters.
     */
    public abstract int indexShard(String id, @Nullable String routing, XContentType sourceType, BytesReference source);

    /**
     * Called when updating a document to generate the shard id that should contain
     * a document with the provided {@code _id} and (optional) {@code _routing}.
     */
    public abstract int updateShard(String id, @Nullable String routing);

    /**
     * Called when deleting a document to generate the shard id that should contain
     * a document with the provided {@code _id} and (optional) {@code _routing}.
     */
    public abstract int deleteShard(String id, @Nullable String routing);

    /**
     * Called when getting a document to generate the shard id that should contain
     * a document with the provided {@code _id} and (optional) {@code _routing}.
     */
    public abstract int getShard(String id, @Nullable String routing);

    /**
     * Collect all of the shard ids that *may* contain documents with the
     * provided {@code routing}. Indices with a {@code routing_partition}
     * will collect more than one shard. Indices without a partition
     * will collect the same shard id as would be returned
     * by {@link #getShard}.
     * <p>
     * Note: This is called for any search-like requests that have a
     * routing specified but <strong>only</strong> if they have a routing
     * specified. If they do not have a routing they just use all shards
     * in the index.
     */
    public abstract void collectSearchShards(String routing, IntConsumer consumer);

    /**
     * Convert a hash generated from an {@code (id, routing}) pair into a
     * shard id.
     */
    protected final int hashToShardId(int hash) {
        return Math.floorMod(hash, routingNumShards) / routingFactor;
    }

    /**
     * Convert a routing value into a hash.
     */
    private static int effectiveRoutingToHash(String effectiveRouting) {
        return Murmur3HashFunction.hash(effectiveRouting);
    }

    private abstract static class IdAndRoutingOnly extends IndexRouting {
        IdAndRoutingOnly(int routingNumShards, int routingFactor) {
            super(routingNumShards, routingFactor);
        }

        protected abstract int shardId(String id, @Nullable String routing);

        @Override
        public int indexShard(String id, @Nullable String routing, XContentType sourceType, BytesReference source) {
            return shardId(id, routing);
        }

        @Override
        public int updateShard(String id, @Nullable String routing) {
            return shardId(id, routing);
        }

        @Override
        public int deleteShard(String id, @Nullable String routing) {
            return shardId(id, routing);
        }

        @Override
        public int getShard(String id, @Nullable String routing) {
            return shardId(id, routing);
        }
    }

    /**
     * Strategy for indices that are not partitioned.
     */
    private static class Unpartitioned extends IdAndRoutingOnly {
        Unpartitioned(int routingNumShards, int routingFactor) {
            super(routingNumShards, routingFactor);
        }

        @Override
        protected int shardId(String id, @Nullable String routing) {
            return hashToShardId(effectiveRoutingToHash(routing == null ? id : routing));
        }

        @Override
        public void collectSearchShards(String routing, IntConsumer consumer) {
            consumer.accept(hashToShardId(effectiveRoutingToHash(routing)));
        }
    }

    /**
     * Strategy for partitioned indices.
     */
    private static class Partitioned extends IdAndRoutingOnly {
        private final int routingPartitionSize;

        Partitioned(int routingNumShards, int routingFactor, int routingPartitionSize) {
            super(routingNumShards, routingFactor);
            this.routingPartitionSize = routingPartitionSize;
        }

        @Override
        protected int shardId(String id, @Nullable String routing) {
            if (routing == null) {
                throw new IllegalArgumentException("A routing value is required for gets from a partitioned index");
            }
            int offset = Math.floorMod(effectiveRoutingToHash(id), routingPartitionSize);
            return hashToShardId(effectiveRoutingToHash(routing) + offset);
        }

        @Override
        public void collectSearchShards(String routing, IntConsumer consumer) {
            int hash = effectiveRoutingToHash(routing);
            for (int i = 0; i < routingPartitionSize; i++) {
                consumer.accept(hashToShardId(hash + i));
            }
        }
    }

    private static class ExtractFromSource extends IndexRouting {
        private final String indexName;
        private final FilterPath[] include;

        ExtractFromSource(int routingNumShards, int routingFactor, String indexName, List<String> routingPaths) {
            super(routingNumShards, routingFactor);
            this.indexName = indexName;
            this.include = FilterPath.compile(Set.copyOf(routingPaths));
        }

        @Override
        public int indexShard(String id, @Nullable String routing, XContentType sourceType, BytesReference source) {
            if (routing != null) {
                throw new IllegalArgumentException(error("indexing with a specified routing"));
            }
            assert Transports.assertNotTransportThread("parsing the _source can get slow");

            try {
                try (
                    XContentParser parser = sourceType.xContent()
                        .createParser(
                            NamedXContentRegistry.EMPTY,
                            DeprecationHandler.THROW_UNSUPPORTED_OPERATION,
                            source.streamInput(),
                            include,
                            null
                        )
                ) {
                    parser.nextToken(); // Move to first token
                    if (parser.currentToken() == null) {
                        throw new IllegalArgumentException("Error extracting routing: source didn't contain any routing fields");
                    }
                    int hash = extractObject(parser);
                    ensureExpectedToken(null, parser.nextToken(), parser);
                    return hashToShardId(hash);
                }
            } catch (IOException | ParsingException e) {
                throw new IllegalArgumentException("Error extracting routing: " + e.getMessage(), e);
            }
        }

        private static int extractObject(XContentParser source) throws IOException {
            ensureExpectedToken(Token.FIELD_NAME, source.nextToken(), source);
            String firstFieldName = source.currentName();
            source.nextToken();
            int firstHash = extractItem(source);
            if (source.currentToken() == Token.END_OBJECT) {
                // Just one routing key in this object
                // Use ^ like Map.Entry's hashcode
                return Murmur3HashFunction.hash(firstFieldName) ^ firstHash;
            }
            List<NameAndHash> hashes = new ArrayList<>();
            hashes.add(new NameAndHash(firstFieldName, firstHash));
            do {
                ensureExpectedToken(Token.FIELD_NAME, source.currentToken(), source);
                String fieldName = source.currentName();
                source.nextToken();
                hashes.add(new NameAndHash(fieldName, extractItem(source)));
            } while (source.currentToken() != Token.END_OBJECT);
            Collections.sort(hashes, Comparator.comparing(nameAndHash -> nameAndHash.name));
            /*
             * This is the same as Arrays.hash(Map.Entry<fieldName, hash>) but we're
             * writing it out so for extra paranoia. Changing this will change how
             * documents are routed and we don't want a jdk update that modifies Arrays
             * or Map.Entry to sneak up on us.
             */
            int hash = 0;
            for (NameAndHash nameAndHash : hashes) {
                int thisHash = Murmur3HashFunction.hash(nameAndHash.name) ^ nameAndHash.hash;
                hash = 31 * hash + thisHash;
            }
            return hash;
        }

        private static int extractItem(XContentParser source) throws IOException {
            if (source.currentToken() == Token.START_OBJECT) {
                int hash = extractObject(source);
                source.nextToken();
                return hash;
            }
            if (source.currentToken() == Token.VALUE_STRING) {
                int hash = Murmur3HashFunction.hash(source.text());
                source.nextToken();
                return hash;
            }
            throw new ParsingException(source.getTokenLocation(), "Routing values must be strings but found [{}]", source.currentToken());
        }

        @Override
        public int updateShard(String id, @Nullable String routing) {
            throw new IllegalArgumentException(error("update"));
        }

        @Override
        public int deleteShard(String id, @Nullable String routing) {
            throw new IllegalArgumentException(error("delete"));
        }

        @Override
        public int getShard(String id, @Nullable String routing) {
            throw new IllegalArgumentException(error("get"));
        }

        @Override
        public void collectSearchShards(String routing, IntConsumer consumer) {
            throw new IllegalArgumentException(error("searching with a specified routing"));
        }

        private String error(String operation) {
            return operation + " is not supported because the destination index [" + indexName + "] is in time series mode";
        }
    }

    private static class NameAndHash {
        private final String name;
        private final int hash;

        NameAndHash(String name, int hash) {
            this.name = name;
            this.hash = hash;
        }
    }
}
