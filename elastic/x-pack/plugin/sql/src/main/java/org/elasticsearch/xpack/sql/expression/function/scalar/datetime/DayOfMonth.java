/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.datetime;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.function.scalar.datetime.DateTimeProcessor.DateTimeExtractor;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo.NodeCtor2;

import java.time.temporal.ChronoField;
import java.util.TimeZone;

/**
 * Extract the day of the month from a datetime.
 */
public class DayOfMonth extends DateTimeFunction {
    public DayOfMonth(Location location, Expression field, TimeZone timeZone) {
        super(location, field, timeZone);
    }

    @Override
    protected NodeCtor2<Expression, TimeZone, DateTimeFunction> ctorForInfo() {
        return DayOfMonth::new;
    }

    @Override
    protected DayOfMonth replaceChild(Expression newChild) {
        return new DayOfMonth(location(), newChild, timeZone());
    }

    @Override
    public String dateTimeFormat() {
        return "d";
    }

    @Override
    protected ChronoField chronoField() {
        return ChronoField.DAY_OF_MONTH;
    }

    @Override
    protected DateTimeExtractor extractor() {
        return DateTimeExtractor.DAY_OF_MONTH;
    }
}
