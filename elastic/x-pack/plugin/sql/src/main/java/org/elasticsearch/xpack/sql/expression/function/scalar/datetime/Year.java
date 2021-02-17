/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.datetime;

import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.tree.NodeInfo.NodeCtor2;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.sql.expression.function.grouping.Histogram;
import org.elasticsearch.xpack.sql.expression.function.scalar.datetime.DateTimeProcessor.DateTimeExtractor;

import java.time.ZoneId;

/**
 * Extract the year from a datetime.
 */
public class Year extends DateTimeHistogramFunction {

    public Year(Source source, Expression field, ZoneId zoneId) {
        super(source, field, zoneId, DateTimeExtractor.YEAR);
    }

    @Override
    protected NodeCtor2<Expression, ZoneId, BaseDateTimeFunction> ctorForInfo() {
        return Year::new;
    }

    @Override
    protected Year replaceChild(Expression newChild) {
        return new Year(source(), newChild, zoneId());
    }

    @Override
    public String calendarInterval() {
        return Histogram.YEAR_INTERVAL;
    }
}
