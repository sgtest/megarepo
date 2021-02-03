/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import com.carrotsearch.hppc.ObjectArrayList;
import org.apache.lucene.document.StoredField;
import org.apache.lucene.search.Query;
import org.apache.lucene.store.ByteArrayDataOutput;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.util.CollectionUtils;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.plain.BytesBinaryIndexFieldData;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.lookup.SearchLookup;

import java.io.IOException;
import java.time.ZoneId;
import java.util.Base64;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;

public class BinaryFieldMapper extends FieldMapper {

    public static final String CONTENT_TYPE = "binary";

    private static BinaryFieldMapper toType(FieldMapper in) {
        return (BinaryFieldMapper) in;
    }

    public static class Builder extends FieldMapper.Builder {

        private final Parameter<Boolean> stored = Parameter.storeParam(m -> toType(m).stored, false);
        private final Parameter<Boolean> hasDocValues = Parameter.docValuesParam(m -> toType(m).hasDocValues,  false);
        private final Parameter<Map<String, String>> meta = Parameter.metaParam();

        public Builder(String name) {
            this(name, false);
        }

        public Builder(String name, boolean hasDocValues) {
            super(name);
            this.hasDocValues.setValue(hasDocValues);
        }

        @Override
        public List<Parameter<?>> getParameters() {
            return List.of(meta, stored, hasDocValues);
        }

        @Override
        public BinaryFieldMapper build(ContentPath contentPath) {
            return new BinaryFieldMapper(name, new BinaryFieldType(buildFullName(contentPath), stored.getValue(),
                hasDocValues.getValue(), meta.getValue()), multiFieldsBuilder.build(this, contentPath), copyTo.build(), this);
        }
    }

    public static final TypeParser PARSER = new TypeParser((n, c) -> new Builder(n));

    public static final class BinaryFieldType extends MappedFieldType {

        private BinaryFieldType(String name, boolean isStored, boolean hasDocValues, Map<String, String> meta) {
            super(name, false, isStored, hasDocValues, TextSearchInfo.NONE, meta);
        }

        public BinaryFieldType(String name) {
            this(name, false, true, Collections.emptyMap());
        }

        @Override
        public String typeName() {
            return CONTENT_TYPE;
        }

        @Override
        public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
            return SourceValueFetcher.identity(name(), context, format);
        }

        @Override
        public DocValueFormat docValueFormat(String format, ZoneId timeZone) {
            return DocValueFormat.BINARY;
        }

        @Override
        public BytesReference valueForDisplay(Object value) {
            if (value == null) {
                return null;
            }

            BytesReference bytes;
            if (value instanceof BytesRef) {
                bytes = new BytesArray((BytesRef) value);
            } else if (value instanceof BytesReference) {
                bytes = (BytesReference) value;
            } else if (value instanceof byte[]) {
                bytes = new BytesArray((byte[]) value);
            } else {
                bytes = new BytesArray(Base64.getDecoder().decode(value.toString()));
            }
            return bytes;
        }

        @Override
        public IndexFieldData.Builder fielddataBuilder(String fullyQualifiedIndexName, Supplier<SearchLookup> searchLookup) {
            failIfNoDocValues();
            return new BytesBinaryIndexFieldData.Builder(name(), CoreValuesSourceType.KEYWORD);
        }

        @Override
        public Query termQuery(Object value, SearchExecutionContext context) {
            throw new IllegalArgumentException("Binary fields do not support searching");
        }
    }

    private final boolean stored;
    private final boolean hasDocValues;

    protected BinaryFieldMapper(String simpleName, MappedFieldType mappedFieldType,
                                MultiFields multiFields, CopyTo copyTo, Builder builder) {
        super(simpleName, mappedFieldType, multiFields, copyTo);
        this.stored = builder.stored.getValue();
        this.hasDocValues = builder.hasDocValues.getValue();
    }

    @Override
    protected void parseCreateField(ParseContext context) throws IOException {
        if (stored == false && hasDocValues == false) {
            return;
        }
        byte[] value = context.parseExternalValue(byte[].class);
        if (value == null) {
            if (context.parser().currentToken() == XContentParser.Token.VALUE_NULL) {
                return;
            } else {
                value = context.parser().binaryValue();
            }
        }
        if (value == null) {
            return;
        }
        if (stored) {
            context.doc().add(new StoredField(fieldType().name(), value));
        }

        if (hasDocValues) {
            CustomBinaryDocValuesField field = (CustomBinaryDocValuesField) context.doc().getByKey(fieldType().name());
            if (field == null) {
                field = new CustomBinaryDocValuesField(fieldType().name(), value);
                context.doc().addWithKey(fieldType().name(), field);
            } else {
                field.add(value);
            }
        } else {
            // Only add an entry to the field names field if the field is stored
            // but has no doc values so exists query will work on a field with
            // no doc values
            createFieldNamesField(context);
        }
    }

    @Override
    public FieldMapper.Builder getMergeBuilder() {
        return new BinaryFieldMapper.Builder(simpleName()).init(this);
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    public static class CustomBinaryDocValuesField extends CustomDocValuesField {

        private final ObjectArrayList<byte[]> bytesList;

        private int totalSize = 0;

        public CustomBinaryDocValuesField(String name, byte[] bytes) {
            super(name);
            bytesList = new ObjectArrayList<>();
            add(bytes);
        }

        public void add(byte[] bytes) {
            bytesList.add(bytes);
            totalSize += bytes.length;
        }

        @Override
        public BytesRef binaryValue() {
            try {
                CollectionUtils.sortAndDedup(bytesList);
                int size = bytesList.size();
                final byte[] bytes = new byte[totalSize + (size + 1) * 5];
                ByteArrayDataOutput out = new ByteArrayDataOutput(bytes);
                out.writeVInt(size);  // write total number of values
                for (int i = 0; i < size; i ++) {
                    final byte[] value = bytesList.get(i);
                    int valueLength = value.length;
                    out.writeVInt(valueLength);
                    out.writeBytes(value, 0, valueLength);
                }
                return new BytesRef(bytes, 0, out.getPosition());
            } catch (IOException e) {
                throw new ElasticsearchException("Failed to get binary value", e);
            }

        }
    }
}
