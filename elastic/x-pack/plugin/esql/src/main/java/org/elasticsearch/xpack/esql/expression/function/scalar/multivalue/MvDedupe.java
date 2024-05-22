/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.esql.expression.function.scalar.multivalue;

import org.elasticsearch.compute.operator.EvalOperator.ExpressionEvaluator;
import org.elasticsearch.compute.operator.mvdedupe.MultivalueDedupe;
import org.elasticsearch.xpack.esql.core.expression.Expression;
import org.elasticsearch.xpack.esql.core.tree.NodeInfo;
import org.elasticsearch.xpack.esql.core.tree.Source;
import org.elasticsearch.xpack.esql.expression.function.Example;
import org.elasticsearch.xpack.esql.expression.function.FunctionInfo;
import org.elasticsearch.xpack.esql.expression.function.Param;
import org.elasticsearch.xpack.esql.planner.PlannerUtils;
import org.elasticsearch.xpack.esql.type.EsqlDataTypes;

import java.util.List;

import static org.elasticsearch.xpack.esql.core.expression.TypeResolutions.isType;

/**
 * Removes duplicate values from a multivalued field.
 */
public class MvDedupe extends AbstractMultivalueFunction {
    // @TODO: add cartesian_point, geo_point, unsigned_long
    @FunctionInfo(
        returnType = { "boolean", "date", "double", "integer", "ip", "keyword", "long", "text", "version" },
        description = "Remove duplicate values from a multivalued field.",
        note = "`MV_DEDUPE` may, but won't always, sort the values in the column.",
        examples = @Example(file = "string", tag = "mv_dedupe")
    )
    public MvDedupe(
        Source source,
        @Param(
            name = "field",
            type = { "boolean", "date", "double", "integer", "ip", "keyword", "long", "text", "version" },
            description = "Multivalue expression."
        ) Expression field
    ) {
        super(source, field);
    }

    @Override
    protected TypeResolution resolveFieldType() {
        return isType(field(), EsqlDataTypes::isRepresentable, sourceText(), null, "representable");
    }

    @Override
    protected ExpressionEvaluator.Factory evaluator(ExpressionEvaluator.Factory fieldEval) {
        return MultivalueDedupe.evaluator(PlannerUtils.toElementType(dataType()), fieldEval);
    }

    @Override
    public Expression replaceChildren(List<Expression> newChildren) {
        return new MvDedupe(source(), newChildren.get(0));
    }

    @Override
    protected NodeInfo<? extends Expression> info() {
        return NodeInfo.create(this, MvDedupe::new, field());
    }
}
