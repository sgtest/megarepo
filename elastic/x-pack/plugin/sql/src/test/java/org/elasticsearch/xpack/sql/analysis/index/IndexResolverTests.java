/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.analysis.index;

import org.elasticsearch.action.fieldcaps.FieldCapabilities;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.sql.type.DataType;
import org.elasticsearch.xpack.sql.type.EsField;
import org.elasticsearch.xpack.sql.type.TypesTests;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Map.Entry;

public class IndexResolverTests extends ESTestCase {

    public void testMergeSameMapping() throws Exception {
        Map<String, EsField> oneMapping = TypesTests.loadMapping("mapping-basic.json", true);
        Map<String, EsField> sameMapping = TypesTests.loadMapping("mapping-basic.json", true);
        assertNotSame(oneMapping, sameMapping);
        assertEquals(oneMapping, sameMapping);

        String wildcard = "*";
        
        IndexResolution resolution = IndexResolver.mergedMapping(wildcard, fromMappings(
                new EsIndex("a", oneMapping),
                new EsIndex("b", sameMapping)));

        assertTrue(resolution.isValid());
        assertEqualsMaps(oneMapping, resolution.get().mapping());
    }

    public void testMergeCompatibleMapping() throws Exception {
        Map<String, EsField> basicMapping = TypesTests.loadMapping("mapping-basic.json", true);
        Map<String, EsField> numericMapping = TypesTests.loadMapping("mapping-numeric.json", true);

        assertNotEquals(basicMapping, numericMapping);

        String wildcard = "*";
        IndexResolution resolution = IndexResolver.mergedMapping(wildcard, fromMappings(
                new EsIndex("basic", basicMapping),
                new EsIndex("numeric", numericMapping)));

        assertTrue(resolution.isValid());
        assertEquals(basicMapping.size() + numericMapping.size(), resolution.get().mapping().size());
    }

    public void testMergeIncompatibleTypes() throws Exception {
        Map<String, EsField> basicMapping = TypesTests.loadMapping("mapping-basic.json", true);
        Map<String, EsField> incompatible = TypesTests.loadMapping("mapping-basic-incompatible.json");

        assertNotEquals(basicMapping, incompatible);

        String wildcard = "*";
        IndexResolution resolution = IndexResolver.mergedMapping(wildcard,
                fromMappings(new EsIndex("basic", basicMapping), new EsIndex("incompatible", incompatible)));

        assertFalse(resolution.isValid());
        MappingException me = expectThrows(MappingException.class, () -> resolution.get());
        assertEquals(
                "[*] points to indices with incompatible mappings;"
                        + " field [gender] is mapped in [2] different ways: [text] in [incompatible], [keyword] in [basic]",
                me.getMessage());
    }

    public void testMergeIncompatibleCapabilities() throws Exception {
        Map<String, EsField> basicMapping = TypesTests.loadMapping("mapping-basic.json", true);
        Map<String, EsField> incompatible = TypesTests.loadMapping("mapping-basic-nodocvalues.json", true);

        assertNotEquals(basicMapping, incompatible);

        String wildcard = "*";
        IndexResolution resolution = IndexResolver.mergedMapping(wildcard,
                fromMappings(new EsIndex("basic", basicMapping), new EsIndex("incompatible", incompatible)));

        assertFalse(resolution.isValid());
        MappingException me = expectThrows(MappingException.class, () -> resolution.get());
        assertEquals(
                "[*] points to indices with incompatible mappings: field [emp_no] is aggregateable except in [incompatible]",
                me.getMessage());
    }

    public void testMultiLevelObjectMappings() throws Exception {
        Map<String, EsField> dottedMapping = TypesTests.loadMapping("mapping-dotted-field.json", true);

        String wildcard = "*";
        IndexResolution resolution = IndexResolver.mergedMapping(wildcard, fromMappings(new EsIndex("a", dottedMapping)));

        assertTrue(resolution.isValid());
        assertEqualsMaps(dottedMapping, resolution.get().mapping());
    }

    public void testMultiLevelNestedMappings() throws Exception {
        Map<String, EsField> nestedMapping = TypesTests.loadMapping("mapping-nested.json", true);
        
        String wildcard = "*";
        IndexResolution resolution = IndexResolver.mergedMapping(wildcard, fromMappings(new EsIndex("a", nestedMapping)));

        assertTrue(resolution.isValid());
        assertEqualsMaps(nestedMapping, resolution.get().mapping());
    }

    private static Map<String, Map<String, FieldCapabilities>> fromMappings(EsIndex... indices) {
        Map<String, Map<String, FieldCapabilities>> merged = new HashMap<>();

        // first pass: create the field caps
        for (EsIndex index : indices) {
            for (EsField field : index.mapping().values()) {
                addFieldCaps(null, field, index.name(), merged);
            }
        }

        // second pass: update indices
        for (Entry<String, Map<String, FieldCapabilities>> entry : merged.entrySet()) {
            String fieldName = entry.getKey();
            Map<String, FieldCapabilities> caps = entry.getValue();
            if (entry.getValue().size() > 1) {
                for (EsIndex index : indices) {
                    EsField field = index.mapping().get(fieldName);
                    UpdateableFieldCapabilities fieldCaps = (UpdateableFieldCapabilities) caps.get(field.getDataType().esType);
                    fieldCaps.indices.add(index.name());
                }
                //TODO: what about nonAgg/SearchIndices?
            }
        }

        return merged;
    }
    
    private static void addFieldCaps(String parent, EsField field, String indexName, Map<String, Map<String, FieldCapabilities>> merged) {
        String fieldName = parent != null ? parent + "." + field.getName() : field.getName();
        Map<String, FieldCapabilities> map = merged.get(fieldName);
        if (map == null) {
            map = new HashMap<>();
            merged.put(fieldName, map);
        }
        FieldCapabilities caps = map.computeIfAbsent(field.getDataType().esType,
                esType -> new UpdateableFieldCapabilities(fieldName, esType,
                isSearchable(field.getDataType()),
                        isAggregatable(field.getDataType())));

        if (!field.isAggregatable()) {
            ((UpdateableFieldCapabilities) caps).nonAggregatableIndices.add(indexName);
        }

        for (EsField nested : field.getProperties().values()) {
            addFieldCaps(fieldName, nested, indexName, merged);
        }
    }

    private static boolean isSearchable(DataType type) {
        return type.isPrimitive();
    }

    private static boolean isAggregatable(DataType type) {
        return type.isNumeric() || type == DataType.KEYWORD || type == DataType.DATE;
    }

    private static class UpdateableFieldCapabilities extends FieldCapabilities {
        List<String> indices = new ArrayList<>();
        List<String> nonSearchableIndices = new ArrayList<>();
        List<String> nonAggregatableIndices = new ArrayList<>();

        UpdateableFieldCapabilities(String name, String type, boolean isSearchable, boolean isAggregatable) {
            super(name, type, isSearchable, isAggregatable);
        }

        @Override
        public String[] indices() {
            return indices.isEmpty() ? null : indices.toArray(new String[indices.size()]);
        }

        @Override
        public String[] nonSearchableIndices() {
            return nonSearchableIndices.isEmpty() ? null : nonSearchableIndices.toArray(new String[nonSearchableIndices.size()]);
        }

        @Override
        public String[] nonAggregatableIndices() {
            return nonAggregatableIndices.isEmpty() ? null : nonAggregatableIndices.toArray(new String[nonAggregatableIndices.size()]);
        }

        @Override
        public String toString() {
            return String.format(Locale.ROOT, "%s,%s->%s", getName(), getType(), indices);
        }
    }

    private static <K, V> void assertEqualsMaps(Map<K, V> left, Map<K, V> right) {
        for (Entry<K, V> entry : left.entrySet()) {
            V rv = right.get(entry.getKey());
            assertEquals(String.format(Locale.ROOT, "Key [%s] has different values", entry.getKey()), entry.getValue(), rv);
        }
    }
}