/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.expression.function.scalar;

import org.elasticsearch.xpack.sql.expression.Expression;
import org.elasticsearch.xpack.sql.expression.FieldAttribute;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinition;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.ProcessorDefinitions;
import org.elasticsearch.xpack.sql.expression.function.scalar.processor.definition.UnaryProcessorDefinition;
import org.elasticsearch.xpack.sql.expression.function.scalar.script.Params;
import org.elasticsearch.xpack.sql.expression.function.scalar.script.ScriptTemplate;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.NodeInfo;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.type.DataTypeConversion;
import org.elasticsearch.xpack.sql.type.DataTypes;

import java.util.Objects;

public class Cast extends UnaryScalarFunction {
    private final DataType dataType;

    public Cast(Location location, Expression field, DataType dataType) {
        super(location, field);
        this.dataType = dataType;
    }

    @Override
    protected NodeInfo<Cast> info() {
        return NodeInfo.create(this, Cast::new, field(), dataType);
    }

    @Override
    protected UnaryScalarFunction replaceChild(Expression newChild) {
        return new Cast(location(), newChild, dataType);
    }

    public DataType from() {
        return field().dataType();
    }

    public DataType to() {
        return dataType;
    }

    @Override
    public DataType dataType() {
        return dataType;
    }

    @Override
    public boolean foldable() {
        return field().foldable();
    }

    @Override
    public Object fold() {
        return DataTypeConversion.convert(field().fold(), dataType);
    }

    @Override
    public boolean nullable() {
        return field().nullable() || DataTypes.isNull(from());
    }

    @Override
    protected TypeResolution resolveType() {
        return DataTypeConversion.canConvert(from(), to()) ?
                TypeResolution.TYPE_RESOLVED :
                    new TypeResolution("Cannot cast %s to %s", from(), to());
    }

    @Override
    protected ScriptTemplate asScriptFrom(ScalarFunctionAttribute scalar) {
        return scalar.script();
    }

    @Override
    protected ScriptTemplate asScriptFrom(FieldAttribute field) {
        return new ScriptTemplate(field.name(), Params.EMPTY, field.dataType());
    }

    @Override
    protected ProcessorDefinition makeProcessorDefinition() {
        return new UnaryProcessorDefinition(location(), this, ProcessorDefinitions.toProcessorDefinition(field()),
                new CastProcessor(DataTypeConversion.conversionFor(from(), to())));
    }

    @Override
    public int hashCode() {
        return Objects.hash(super.hashCode(), dataType);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }
        if (obj == null || obj.getClass() != getClass()) {
            return false;
        }
        Cast other = (Cast) obj;
        return Objects.equals(dataType, other.dataType())
            && Objects.equals(field(), other.field());
    }

    @Override
    public String toString() {
        return functionName() + "(" + field().toString() + " AS " + to().sqlName() + ")#" + id();
    }
}