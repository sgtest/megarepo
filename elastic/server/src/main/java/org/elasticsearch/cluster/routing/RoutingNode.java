/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.routing;

import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.util.Maps;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.shard.ShardId;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.function.Predicate;
import java.util.stream.Collectors;

/**
 * A {@link RoutingNode} represents a cluster node associated with a single {@link DiscoveryNode} including all shards
 * that are hosted on that nodes. Each {@link RoutingNode} has a unique node id that can be used to identify the node.
 */
public class RoutingNode implements Iterable<ShardRouting> {

    private final String nodeId;

    @Nullable
    private final DiscoveryNode node;

    private final LinkedHashMap<ShardId, ShardRouting> shards; // LinkedHashMap to preserve order

    private final LinkedHashSet<ShardRouting> initializingShards;

    private final LinkedHashSet<ShardRouting> relocatingShards;

    private final Map<Index, Set<ShardRouting>> shardsByIndex;

    /**
     * @param nodeId    node id of this routing node
     * @param node      discovery node for this routing node
     * @param sizeGuess estimate for the number of shards that will be added to this instance to save re-hashing on subsequent calls to
     *                  {@link #add(ShardRouting)}
     */
    RoutingNode(String nodeId, @Nullable DiscoveryNode node, int sizeGuess) {
        this.nodeId = nodeId;
        this.node = node;
        this.shards = Maps.newLinkedHashMapWithExpectedSize(sizeGuess);
        this.relocatingShards = new LinkedHashSet<>();
        this.initializingShards = new LinkedHashSet<>();
        this.shardsByIndex = Maps.newHashMapWithExpectedSize(sizeGuess);
        assert invariant();
    }

    private RoutingNode(RoutingNode original) {
        this.nodeId = original.nodeId;
        this.node = original.node;
        this.shards = new LinkedHashMap<>(original.shards);
        this.relocatingShards = new LinkedHashSet<>(original.relocatingShards);
        this.initializingShards = new LinkedHashSet<>(original.initializingShards);
        this.shardsByIndex = Maps.newMapWithExpectedSize(original.shardsByIndex.size());
        for (Map.Entry<Index, Set<ShardRouting>> entry : original.shardsByIndex.entrySet()) {
            shardsByIndex.put(entry.getKey(), new HashSet<>(entry.getValue()));
        }
        assert invariant();
    }

    RoutingNode copy() {
        return new RoutingNode(this);
    }

    @Override
    public Iterator<ShardRouting> iterator() {
        return Collections.unmodifiableCollection(shards.values()).iterator();
    }

    /**
     * Returns the nodes {@link DiscoveryNode}.
     *
     * @return discoveryNode of this node
     */
    @Nullable
    public DiscoveryNode node() {
        return this.node;
    }

    @Nullable
    public ShardRouting getByShardId(ShardId id) {
        return shards.get(id);
    }

    public boolean hasIndex(Index index) {
        return shardsByIndex.containsKey(index);
    }

    /**
     * Get the id of this node
     * @return id of the node
     */
    public String nodeId() {
        return this.nodeId;
    }

    public int size() {
        return shards.size();
    }

    /**
     * Add a new shard to this node
     * @param shard Shard to create on this Node
     */
    void add(ShardRouting shard) {
        assert invariant();
        final ShardRouting existing = shards.putIfAbsent(shard.shardId(), shard);
        if (existing != null) {
            final IllegalStateException e = new IllegalStateException(
                "Trying to add a shard "
                    + shard.shardId()
                    + " to a node ["
                    + nodeId
                    + "] where it already exists. current ["
                    + shards.get(shard.shardId())
                    + "]. new ["
                    + shard
                    + "]"
            );
            assert false : e;
            throw e;
        }

        if (shard.initializing()) {
            initializingShards.add(shard);
        } else if (shard.relocating()) {
            relocatingShards.add(shard);
        }
        shardsByIndex.computeIfAbsent(shard.index(), k -> new HashSet<>()).add(shard);
        assert invariant();
    }

    void update(ShardRouting oldShard, ShardRouting newShard) {
        assert invariant();
        if (shards.containsKey(oldShard.shardId()) == false) {
            // Shard was already removed by routing nodes iterator
            // TODO: change caller logic in RoutingNodes so that this check can go away
            return;
        }
        ShardRouting previousValue = shards.put(newShard.shardId(), newShard);
        assert previousValue == oldShard : "expected shard " + previousValue + " but was " + oldShard;

        if (oldShard.initializing()) {
            boolean exist = initializingShards.remove(oldShard);
            assert exist : "expected shard " + oldShard + " to exist in initializingShards";
        } else if (oldShard.relocating()) {
            boolean exist = relocatingShards.remove(oldShard);
            assert exist : "expected shard " + oldShard + " to exist in relocatingShards";
        }
        final Set<ShardRouting> byIndex = shardsByIndex.get(oldShard.index());
        byIndex.remove(oldShard);
        byIndex.add(newShard);
        if (newShard.initializing()) {
            initializingShards.add(newShard);
        } else if (newShard.relocating()) {
            relocatingShards.add(newShard);
        }
        assert invariant();
    }

    void remove(ShardRouting shard) {
        assert invariant();
        ShardRouting previousValue = shards.remove(shard.shardId());
        assert previousValue == shard : "expected shard " + previousValue + " but was " + shard;
        if (shard.initializing()) {
            boolean exist = initializingShards.remove(shard);
            assert exist : "expected shard " + shard + " to exist in initializingShards";
        } else if (shard.relocating()) {
            boolean exist = relocatingShards.remove(shard);
            assert exist : "expected shard " + shard + " to exist in relocatingShards";
        }
        final Set<ShardRouting> byIndex = shardsByIndex.get(shard.index());
        byIndex.remove(shard);
        if (byIndex.isEmpty()) {
            shardsByIndex.remove(shard.index());
        }
        assert invariant();
    }

    /**
     * Determine the number of shards with a specific state
     * @param states set of states which should be counted
     * @return number of shards
     */
    public int numberOfShardsWithState(ShardRoutingState... states) {
        if (states.length == 1) {
            if (states[0] == ShardRoutingState.INITIALIZING) {
                return initializingShards.size();
            } else if (states[0] == ShardRoutingState.RELOCATING) {
                return relocatingShards.size();
            }
        }

        int count = 0;
        for (ShardRouting shardEntry : this) {
            for (ShardRoutingState state : states) {
                if (shardEntry.state() == state) {
                    count++;
                }
            }
        }
        return count;
    }

    /**
     * Determine the shards with a specific state
     * @param state state which should be listed
     * @return List of shards
     */
    public List<ShardRouting> shardsWithState(ShardRoutingState state) {
        if (state == ShardRoutingState.INITIALIZING) {
            return new ArrayList<>(initializingShards);
        } else if (state == ShardRoutingState.RELOCATING) {
            return new ArrayList<>(relocatingShards);
        }
        List<ShardRouting> shards = new ArrayList<>();
        for (ShardRouting shardEntry : this) {
            if (shardEntry.state() == state) {
                shards.add(shardEntry);
            }
        }
        return shards;
    }

    private static final ShardRouting[] EMPTY_SHARD_ROUTING_ARRAY = new ShardRouting[0];

    public ShardRouting[] initializing() {
        return initializingShards.toArray(EMPTY_SHARD_ROUTING_ARRAY);
    }

    public ShardRouting[] relocating() {
        return relocatingShards.toArray(EMPTY_SHARD_ROUTING_ARRAY);
    }

    /**
     * Determine the shards of an index with a specific state
     * @param index id of the index
     * @param states set of states which should be listed
     * @return a list of shards
     */
    public List<ShardRouting> shardsWithState(String index, ShardRoutingState... states) {
        List<ShardRouting> shards = new ArrayList<>();

        if (states.length == 1) {
            if (states[0] == ShardRoutingState.INITIALIZING) {
                for (ShardRouting shardEntry : initializingShards) {
                    if (shardEntry.getIndexName().equals(index) == false) {
                        continue;
                    }
                    shards.add(shardEntry);
                }
                return shards;
            } else if (states[0] == ShardRoutingState.RELOCATING) {
                for (ShardRouting shardEntry : relocatingShards) {
                    if (shardEntry.getIndexName().equals(index) == false) {
                        continue;
                    }
                    shards.add(shardEntry);
                }
                return shards;
            }
        }

        for (ShardRouting shardEntry : this) {
            if (shardEntry.getIndexName().equals(index) == false) {
                continue;
            }
            for (ShardRoutingState state : states) {
                if (shardEntry.state() == state) {
                    shards.add(shardEntry);
                }
            }
        }
        return shards;
    }

    /**
     * The number of shards on this node that will not be eventually relocated.
     */
    public int numberOfOwningShards() {
        return shards.size() - relocatingShards.size();
    }

    public int numberOfOwningShardsForIndex(final Index index) {
        final Set<ShardRouting> shardRoutings = shardsByIndex.get(index);
        if (shardRoutings == null) {
            return 0;
        } else {
            return Math.toIntExact(shardRoutings.stream().filter(Predicate.not(ShardRouting::relocating)).count());
        }
    }

    public String prettyPrint() {
        StringBuilder sb = new StringBuilder();
        sb.append("-----node_id[").append(nodeId).append("][").append(node == null ? "X" : "V").append("]\n");
        for (ShardRouting entry : shards.values()) {
            sb.append("--------").append(entry.shortSummary()).append('\n');
        }
        return sb.toString();
    }

    public String toString() {
        StringBuilder sb = new StringBuilder();
        sb.append("routingNode ([");
        if (node != null) {
            sb.append(node.getName());
            sb.append("][");
            sb.append(node.getId());
            sb.append("][");
            sb.append(node.getHostName());
            sb.append("][");
            sb.append(node.getHostAddress());
        } else {
            sb.append("null");
        }
        sb.append("], [");
        sb.append(shards.size());
        sb.append(" assigned shards])");
        return sb.toString();
    }

    public List<ShardRouting> copyShards() {
        return new ArrayList<>(shards.values());
    }

    public boolean isEmpty() {
        return shards.isEmpty();
    }

    private boolean invariant() {
        // initializingShards must consistent with that in shards
        Collection<ShardRouting> shardRoutingsInitializing = shards.values().stream().filter(ShardRouting::initializing).toList();
        assert initializingShards.size() == shardRoutingsInitializing.size();
        assert initializingShards.containsAll(shardRoutingsInitializing);

        // relocatingShards must consistent with that in shards
        Collection<ShardRouting> shardRoutingsRelocating = shards.values().stream().filter(ShardRouting::relocating).toList();
        assert relocatingShards.size() == shardRoutingsRelocating.size();
        assert relocatingShards.containsAll(shardRoutingsRelocating);

        final Map<Index, Set<ShardRouting>> shardRoutingsByIndex = shards.values()
            .stream()
            .collect(Collectors.groupingBy(ShardRouting::index, Collectors.toSet()));
        assert shardRoutingsByIndex.equals(shardsByIndex);

        return true;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        RoutingNode that = (RoutingNode) o;
        return nodeId.equals(that.nodeId) && Objects.equals(node, that.node) && shards.equals(that.shards);
    }

    @Override
    public int hashCode() {
        return Objects.hash(nodeId, node, shards);
    }
}
