/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.profiler;

import org.elasticsearch.xcontent.ObjectPath;
import org.elasticsearch.xcontent.ToXContentObject;
import org.elasticsearch.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;

final class StackFrame implements ToXContentObject {
    List<String> fileName;
    List<String> functionName;
    List<Integer> functionOffset;
    List<Integer> lineNumber;
    List<Integer> sourceType;

    StackFrame(Object fileName, Object functionName, Object functionOffset, Object lineNumber, Object sourceType) {
        this.fileName = listOf(fileName);
        this.functionName = listOf(functionName);
        this.functionOffset = listOf(functionOffset);
        this.lineNumber = listOf(lineNumber);
        this.sourceType = listOf(sourceType);
    }

    @SuppressWarnings("unchecked")
    private static <T> List<T> listOf(Object o) {
        if (o instanceof List) {
            return (List<T>) o;
        } else if (o != null) {
            return List.of((T) o);
        } else {
            return Collections.emptyList();
        }
    }

    public static StackFrame fromSource(Map<String, Object> source) {
        // stack frames may either be stored with synthetic source or regular one
        // which results either in a nested or flat document structure.

        if (source.containsKey("Stackframe")) {
            // synthetic source
            return new StackFrame(
                ObjectPath.eval("Stackframe.file.name", source),
                ObjectPath.eval("Stackframe.function.name", source),
                ObjectPath.eval("Stackframe.function.offset", source),
                ObjectPath.eval("Stackframe.line.number", source),
                ObjectPath.eval("Stackframe.source.type", source)
            );
        } else {
            // regular source
            return new StackFrame(
                source.get("Stackframe.file.name"),
                source.get("Stackframe.function.name"),
                source.get("Stackframe.function.offset"),
                source.get("Stackframe.line.number"),
                source.get("Stackframe.source.type")
            );
        }
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field("file_name", this.fileName);
        builder.field("function_name", this.functionName);
        builder.field("function_offset", this.functionOffset);
        builder.field("line_number", this.lineNumber);
        builder.field("source_type", this.sourceType);
        builder.endObject();
        return builder;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        StackFrame that = (StackFrame) o;
        return Objects.equals(fileName, that.fileName)
            && Objects.equals(functionName, that.functionName)
            && Objects.equals(functionOffset, that.functionOffset)
            && Objects.equals(lineNumber, that.lineNumber)
            && Objects.equals(sourceType, that.sourceType);
    }

    @Override
    public int hashCode() {
        return Objects.hash(fileName, functionName, functionOffset, lineNumber, sourceType);
    }
}
