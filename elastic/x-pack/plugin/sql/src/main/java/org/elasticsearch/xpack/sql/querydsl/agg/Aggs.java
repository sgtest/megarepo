/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.querydsl.agg;

import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeValuesSourceBuilder;
import org.elasticsearch.search.aggregations.bucket.filter.FiltersAggregationBuilder;
import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.querydsl.container.Sort.Direction;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Objects;

import static java.util.Collections.emptyList;
import static org.elasticsearch.index.query.QueryBuilders.matchAllQuery;
import static org.elasticsearch.xpack.sql.util.CollectionUtils.combine;
import static org.elasticsearch.xpack.sql.util.StringUtils.EMPTY;

/**
 * SQL Aggregations associated with a query.
 *
 * This class maps the SQL GroupBy's (and co) to ES composite agg.
 * While the composite agg doesn't require a dedicated structure, for folding purposes, this structure
 * tracks the relationship between each key and its sub-aggs or pipelines.
 * 
 * Since sub-aggs can only refer to their group key and these are on the root-level, the tree can have at most
 * 2 levels - the grouping and its sub-aggs.
 * 
 * In case no group is specified (which maps to the default group in SQL), due to ES nature a 'dummy' filter agg
 * is used.
 */
public class Aggs {

    public static final String ROOT_GROUP_NAME = "groupby";

    public static final GroupByKey IMPLICIT_GROUP_KEY = new GroupByKey(ROOT_GROUP_NAME, EMPTY, null) {

        @Override
        public CompositeValuesSourceBuilder<?> asValueSource() {
            throw new SqlIllegalArgumentException("Default group does not translate to an aggregation");
        }

        @Override
        protected GroupByKey copy(String id, String fieldName, Direction direction) {
            return this;
        }
    };

    private final List<GroupByKey> groups;
    private final List<LeafAgg> metricAggs;
    private final List<PipelineAgg> pipelineAggs;

    public Aggs() {
        this(emptyList(), emptyList(), emptyList());
    }

    public Aggs(List<GroupByKey> groups, List<LeafAgg> metricAggs, List<PipelineAgg> pipelineAggs) {
        this.groups = groups;

        this.metricAggs = metricAggs;
        this.pipelineAggs = pipelineAggs;
    }

    public List<GroupByKey> groups() {
        return groups;
    }

    public AggregationBuilder asAggBuilder() {
        AggregationBuilder rootGroup = null;

        if (groups.isEmpty() && metricAggs.isEmpty()) {
            return null;
        }

        // if there's a group, move everything under the composite agg
        if (!groups.isEmpty()) {
            List<CompositeValuesSourceBuilder<?>> keys = new ArrayList<>(groups.size());
            // first iterate to compute the sources
            for (GroupByKey key : groups) {
                keys.add(key.asValueSource());
            }

            rootGroup = new CompositeAggregationBuilder(ROOT_GROUP_NAME, keys);

        } else {
            rootGroup = new FiltersAggregationBuilder(ROOT_GROUP_NAME, matchAllQuery());
        }

        for (LeafAgg agg : metricAggs) {
            rootGroup.subAggregation(agg.toBuilder());
        }

        for (PipelineAgg agg : pipelineAggs) {
            rootGroup.subAggregation(agg.toBuilder());
        }

        return rootGroup;
    }

    public boolean useImplicitGroupBy() {
        return groups.isEmpty();
    }

    public Aggs addGroups(Collection<GroupByKey> groups) {
        return new Aggs(combine(this.groups, groups), metricAggs, pipelineAggs);
    }

    public Aggs addAgg(LeafAgg agg) {
        return new Aggs(groups, combine(metricAggs, agg), pipelineAggs);
    }

    public Aggs addAgg(PipelineAgg pipelineAgg) {
        return new Aggs(groups, metricAggs, combine(pipelineAggs, pipelineAgg));
    }

    public GroupByKey findGroupForAgg(String groupOrAggId) {
        for (GroupByKey group : this.groups) {
            if (groupOrAggId.equals(group.id())) {
                return group;
            }
        }

        // maybe it's the default group agg ?
        for (Agg agg : metricAggs) {
            if (groupOrAggId.equals(agg.id())) {
                return IMPLICIT_GROUP_KEY;
            }
        }

        return null;
    }

    public Aggs updateGroup(GroupByKey group) {
        List<GroupByKey> groups = new ArrayList<>(this.groups);
        for (int i = 0; i < groups.size(); i++) {
            GroupByKey g = groups.get(i);
            if (group.id().equals(g.id())) {
                groups.set(i, group);
                return with(groups);
            }
        }
        throw new SqlIllegalArgumentException("Could not find group named {}", group.id());
    }

    public Aggs with(List<GroupByKey> groups) {
        return new Aggs(groups, metricAggs, pipelineAggs);
    }

    @Override
    public int hashCode() {
        return Objects.hash(groups, metricAggs, pipelineAggs);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        Aggs other = (Aggs) obj;
        return Objects.equals(groups, other.groups)
                && Objects.equals(metricAggs, other.metricAggs)
                && Objects.equals(pipelineAggs, other.pipelineAggs);
                
    }
}