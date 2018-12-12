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

import java.util.TimeZone;

/**
 * Extract the day of the week (following the ISO standard) from a datetime. 1 is Monday, 2 is Tuesday, etc.
 */
public class IsoDayOfWeek extends DateTimeFunction {
    public IsoDayOfWeek(Location location, Expression field, TimeZone timeZone) {
        super(location, field, timeZone, DateTimeExtractor.ISO_DAY_OF_WEEK);
    }

    @Override
    protected NodeCtor2<Expression, TimeZone, BaseDateTimeFunction> ctorForInfo() {
        return IsoDayOfWeek::new;
    }

    @Override
    protected IsoDayOfWeek replaceChild(Expression newChild) {
        return new IsoDayOfWeek(location(), newChild, timeZone());
    }

    @Override
    public String dateTimeFormat() {
        return "e";
    }
}
