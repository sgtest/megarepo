/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar.string;

import org.elasticsearch.xpack.ql.expression.Expression;
import org.elasticsearch.xpack.ql.expression.Expressions.ParamOrdinal;
import org.elasticsearch.xpack.ql.expression.FieldAttribute;
import org.elasticsearch.xpack.ql.expression.function.scalar.BinaryScalarFunction;
import org.elasticsearch.xpack.ql.expression.gen.script.ScriptTemplate;
import org.elasticsearch.xpack.ql.expression.gen.script.Scripts;
import org.elasticsearch.xpack.ql.tree.Source;

import java.util.Locale;
import java.util.Objects;
import java.util.function.BiFunction;

import static org.elasticsearch.xpack.ql.expression.TypeResolutions.isStringAndExact;
import static org.elasticsearch.xpack.ql.expression.gen.script.ParamsBuilder.paramsBuilder;

/**
 * Base class for binary functions that have the first parameter a string, the second parameter a number
 * or a string and the result can be a string or a number.
 */
public abstract class BinaryStringFunction<T,R> extends BinaryScalarFunction {

    protected BinaryStringFunction(Source source, Expression left, Expression right) {
        super(source, left, right);
    }

    /*
     * the operation the binary function handles can receive one String argument, a number or String as second argument
     * and it can return a number or a String. The BiFunction below is the base operation for the subsequent implementations.
     * T is the second argument, R is the result of applying the operation.
     */
    protected abstract BiFunction<String, T, R> operation();

    @Override
    protected TypeResolution resolveType() {
        if (!childrenResolved()) {
            return new TypeResolution("Unresolved children");
        }

        TypeResolution resolution = isStringAndExact(left(), sourceText(), ParamOrdinal.FIRST);
        if (resolution.unresolved()) {
            return resolution;
        }

        return resolveSecondParameterInputType(right());
    }

    protected abstract TypeResolution resolveSecondParameterInputType(Expression e);

    @Override
    public Object fold() {
        @SuppressWarnings("unchecked")
        T fold = (T) right().fold();
        return operation().apply((String) left().fold(), fold);
    }

    @Override
    protected String scriptMethodName() {
        return operation().toString().toLowerCase(Locale.ROOT);
    }

    @Override
    public ScriptTemplate scriptWithField(FieldAttribute field) {
        return new ScriptTemplate(processScript(Scripts.DOC_VALUE),
                paramsBuilder().variable(field.exactAttribute().name()).build(),
                dataType());
    }

    @Override
    public int hashCode() {
        return Objects.hash(left(), right());
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null || obj.getClass() != getClass()) {
            return false;
        }
        BinaryStringFunction<?,?> other = (BinaryStringFunction<?,?>) obj;
        return Objects.equals(other.left(), left())
            && Objects.equals(other.right(), right());
    }
}
