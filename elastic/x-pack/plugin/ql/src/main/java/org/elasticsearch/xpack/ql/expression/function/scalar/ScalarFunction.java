/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ql.expression.function.scalar;

import java.time.OffsetTime;
import java.time.ZonedDateTime;
import java.util.List;

import org.elasticsearch.xpack.ql.QlIllegalArgumentException;
import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.FieldAttribute;
import org.elasticsearch.xpack.ql.expression.function.Function;
import org.elasticsearch.xpack.ql.expression.function.aggregate.AggregateFunction;
import org.elasticsearch.xpack.ql.expression.function.grouping.GroupingFunction;
import org.elasticsearch.xpack.ql.expression.gen.script.ScriptTemplate;
import org.elasticsearch.xpack.ql.expression.gen.script.Scripts;
import org.elasticsearch.xpack.ql.tree.Source;
import org.elasticsearch.xpack.ql.type.DataTypes;
import org.elasticsearch.xpack.ql.util.DateUtils;

import static java.util.Collections.emptyList;
import static org.elasticsearch.xpack.ql.expression.gen.script.ParamsBuilder.paramsBuilder;
import static org.elasticsearch.xpack.ql.type.DataTypes.DATETIME_NANOS;

/**
 * A {@code ScalarFunction} is a {@code Function} that takes values from some
 * operation and converts each to another value. An example would be
 * {@code ABS()}, which takes one value at a time, applies a function to the
 * value (abs) and returns a new value.
 */
public abstract class ScalarFunction extends Function {

    protected ScalarFunction(Source source) {
        super(source, emptyList());
    }

    protected ScalarFunction(Source source, List<Expression> fields) {
        super(source, fields);
    }

    //
    // Script generation
    //
    public ScriptTemplate asScript(Expression exp) {
        if (exp.foldable()) {
            return scriptWithFoldable(exp);
        }

        if (exp instanceof FieldAttribute) {
            return scriptWithField((FieldAttribute) exp);
        }

        if (exp instanceof ScalarFunction) {
            return scriptWithScalar((ScalarFunction) exp);
        }

        if (exp instanceof AggregateFunction) {
            return scriptWithAggregate((AggregateFunction) exp);
        }

        if (exp instanceof GroupingFunction) {
            return scriptWithGrouping((GroupingFunction) exp);
        }
        throw new QlIllegalArgumentException("Cannot evaluate script for expression {}", exp);
    }

    protected ScriptTemplate scriptWithFoldable(Expression foldable) {
        Object fold = foldable.fold();

        // FIXME: this needs to be refactored
        //
        // Custom type handling
        //

        // wrap intervals with dedicated methods for serialization
        if (fold instanceof ZonedDateTime) {
            ZonedDateTime zdt = (ZonedDateTime) fold;
            return new ScriptTemplate(processScript("{sql}.asDateTime({})"), paramsBuilder().variable(DateUtils.toString(zdt)).build(),
                    dataType());
        }

        if (fold instanceof IntervalScripting) {
            IntervalScripting is = (IntervalScripting) fold;
            return new ScriptTemplate(processScript(is.script()), paramsBuilder().variable(is.value()).variable(is.typeName()).build(),
                    dataType());
        }

        if (fold instanceof OffsetTime) {
            OffsetTime ot = (OffsetTime) fold;
            return new ScriptTemplate(processScript("{sql}.asTime({})"), paramsBuilder().variable(ot.toString()).build(), dataType());
        }

        if (fold != null && fold.getClass().getSimpleName().equals("GeoShape")) {
            return new ScriptTemplate(processScript("{sql}.stWktToSql({})"), paramsBuilder().variable(fold.toString()).build(), dataType());
        }

        return new ScriptTemplate(processScript("{}"),
                paramsBuilder().variable(fold).build(),
                dataType());
    }

    protected ScriptTemplate scriptWithScalar(ScalarFunction scalar) {
        ScriptTemplate nested = scalar.asScript();
        return new ScriptTemplate(processScript(nested.template()),
                paramsBuilder().script(nested.params()).build(),
                dataType());
    }

    protected ScriptTemplate scriptWithAggregate(AggregateFunction aggregate) {
        String template = basicTemplate(aggregate);
        return new ScriptTemplate(processScript(template),
                paramsBuilder().agg(aggregate).build(),
                dataType());
    }

    // This method isn't actually used at the moment, since there is no grouping function (ie HISTOGRAM)
    // that currently results in a script being generated
    protected ScriptTemplate scriptWithGrouping(GroupingFunction grouping) {
        String template = basicTemplate(grouping);
        return new ScriptTemplate(processScript(template),
                paramsBuilder().grouping(grouping).build(),
                dataType());
    }

    // FIXME: this needs to be refactored to account for different datatypes in different projects (ie DATE from SQL)
    private String basicTemplate(Function function) {
        if (function.dataType().name().equals("DATE") || function.dataType() == DataTypes.DATETIME ||
            // Aggregations on date_nanos are returned as double
            (function instanceof AggregateFunction && ((AggregateFunction) function).field().dataType() == DATETIME_NANOS)) {

            return "{sql}.asDateTime({})";
        } else {
            return "{}";
        }
    }

    protected ScriptTemplate scriptWithField(FieldAttribute field) {
        return new ScriptTemplate(processScript(Scripts.DOC_VALUE),
                paramsBuilder().variable(field.name()).build(),
                dataType());
    }

    protected String processScript(String script) {
        return formatTemplate(script);
    }

    protected String formatTemplate(String template) {
        return Scripts.formatTemplate(template);
    }
}
