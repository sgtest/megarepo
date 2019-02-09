/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression;

import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.expression.Expression.TypeResolution;
import org.elasticsearch.xpack.sql.expression.gen.pipeline.Pipe;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.type.DataTypes;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.StringJoiner;
import java.util.function.Predicate;

import static java.util.Collections.emptyList;
import static java.util.Collections.emptyMap;
import static org.elasticsearch.common.logging.LoggerMessageFormat.format;
import static org.elasticsearch.xpack.sql.type.DataType.BOOLEAN;

public final class Expressions {

    public enum ParamOrdinal {
        DEFAULT,
        FIRST,
        SECOND,
        THIRD,
        FOURTH
    }

    private Expressions() {}

    public static NamedExpression wrapAsNamed(Expression exp) {
        return exp instanceof NamedExpression ? (NamedExpression) exp : new Alias(exp.source(), exp.sourceText(), exp);
    }

    public static List<Attribute> asAttributes(List<? extends NamedExpression> named) {
        if (named.isEmpty()) {
            return emptyList();
        }
        List<Attribute> list = new ArrayList<>(named.size());
        for (NamedExpression exp : named) {
            list.add(exp.toAttribute());
        }
        return list;
    }

    public static AttributeMap<Expression> asAttributeMap(List<? extends NamedExpression> named) {
        if (named.isEmpty()) {
            return new AttributeMap<>(emptyMap());
        }

        AttributeMap<Expression> map = new AttributeMap<>();
        for (NamedExpression exp : named) {
            map.add(exp.toAttribute(), exp);
        }
        return map;
    }

    public static boolean anyMatch(List<? extends Expression> exps, Predicate<? super Expression> predicate) {
        for (Expression exp : exps) {
            if (exp.anyMatch(predicate)) {
                return true;
            }
        }
        return false;
    }

    public static boolean match(List<? extends Expression> exps, Predicate<? super Expression> predicate) {
        for (Expression exp : exps) {
            if (predicate.test(exp)) {
                return true;
            }
        }
        return false;
    }

    public static Nullability nullable(List<? extends Expression> exps) {
        return Nullability.and(exps.stream().map(Expression::nullable).toArray(Nullability[]::new));
    }

    public static boolean foldable(List<? extends Expression> exps) {
        for (Expression exp : exps) {
            if (!exp.foldable()) {
                return false;
            }
        }
        return true;
    }

    public static AttributeSet references(List<? extends Expression> exps) {
        if (exps.isEmpty()) {
            return AttributeSet.EMPTY;
        }

        AttributeSet set = new AttributeSet();
        for (Expression exp : exps) {
            set.addAll(exp.references());
        }
        return set;
    }

    public static String name(Expression e) {
        return e instanceof NamedExpression ? ((NamedExpression) e).name() : e.nodeName();
    }

    public static boolean isNull(Expression e) {
        return e.dataType() == DataType.NULL || (e.foldable() && e.fold() == null);
    }

    public static List<String> names(Collection<? extends Expression> e) {
        List<String> names = new ArrayList<>(e.size());
        for (Expression ex : e) {
            names.add(name(ex));
        }

        return names;
    }

    public static Attribute attribute(Expression e) {
        if (e instanceof NamedExpression) {
            return ((NamedExpression) e).toAttribute();
        }
        if (e != null && e.foldable()) {
            return Literal.of(e).toAttribute();
        }
        return null;
    }

    public static boolean equalsAsAttribute(Expression left, Expression right) {
        if (!left.semanticEquals(right)) {
            Attribute l = attribute(left);
            return (l != null && l.semanticEquals(attribute(right)));
        }
        return true;
    }

    public static Pipe pipe(Expression e) {
        if (e instanceof NamedExpression) {
            return ((NamedExpression) e).asPipe();
        }
        throw new SqlIllegalArgumentException("Cannot create pipe for {}", e);
    }

    public static List<Pipe> pipe(List<Expression> expressions) {
        List<Pipe> pipes = new ArrayList<>(expressions.size());
        for (Expression e : expressions) {
            pipes.add(pipe(e));
        }
        return pipes;
    }

    public static TypeResolution typeMustBeBoolean(Expression e, String operationName, ParamOrdinal paramOrd) {
        return typeMustBe(e, dt -> dt == BOOLEAN, operationName, paramOrd, "boolean");
    }

    public static TypeResolution typeMustBeInteger(Expression e, String operationName, ParamOrdinal paramOrd) {
        return typeMustBe(e, DataType::isInteger, operationName, paramOrd, "integer");
    }

    public static TypeResolution typeMustBeNumeric(Expression e, String operationName, ParamOrdinal paramOrd) {
        return typeMustBe(e, DataType::isNumeric, operationName, paramOrd, "numeric");
    }

    public static TypeResolution typeMustBeString(Expression e, String operationName, ParamOrdinal paramOrd) {
        return typeMustBe(e, DataType::isString, operationName, paramOrd, "string");
    }

    public static TypeResolution typeMustBeDate(Expression e, String operationName, ParamOrdinal paramOrd) {
        return typeMustBe(e, DataType::isDateBased, operationName, paramOrd, "date", "datetime");
    }

    public static TypeResolution typeMustBeNumericOrDate(Expression e, String operationName, ParamOrdinal paramOrd) {
        return typeMustBe(e, dt -> dt.isNumeric() || dt.isDateBased(), operationName, paramOrd, "date", "datetime", "numeric");
    }

    public static TypeResolution typeMustBe(Expression e,
                                            Predicate<DataType> predicate,
                                            String operationName,
                                            ParamOrdinal paramOrd,
                                            String... acceptedTypes) {
        return predicate.test(e.dataType()) || DataTypes.isNull(e.dataType())?
            TypeResolution.TYPE_RESOLVED :
                new TypeResolution(format(null, "[{}]{} argument must be [{}], found value [{}] type [{}]",
                operationName,
                paramOrd == null || paramOrd == ParamOrdinal.DEFAULT ? "" : " " + paramOrd.name().toLowerCase(Locale.ROOT),
                acceptedTypesForErrorMsg(acceptedTypes),
                Expressions.name(e),
                e.dataType().typeName));
    }

    private static String acceptedTypesForErrorMsg(String... acceptedTypes) {
        StringJoiner sj = new StringJoiner(", ");
        for (int i = 0; i < acceptedTypes.length - 1; i++) {
            sj.add(acceptedTypes[i]);
        }
        if (acceptedTypes.length > 1) {
            return sj.toString() + " or " + acceptedTypes[acceptedTypes.length - 1];
        } else {
            return acceptedTypes[0];
        }
    }
}
