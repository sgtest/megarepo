/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import java.util.Map;
import java.util.stream.Stream;

/**
 * A container that supports looking up field types for 'dynamic key' fields ({@link DynamicKeyFieldMapper}).
 *
 * Compared to standard fields, 'dynamic key' fields require special handling. Given a field name of the form
 * 'path_to_field.path_to_key', the container will dynamically return a new {@link MappedFieldType} that is
 * suitable for performing searches on the sub-key.
 *
 * Note: we anticipate that 'flattened' fields will be the only implementation {@link DynamicKeyFieldMapper}.
 * Flattened object fields live in the 'mapper-flattened' module.
 */
class DynamicKeyFieldTypeLookup {
    private final Map<String, DynamicKeyFieldMapper> mappers;
    private final Map<String, String> aliasToConcreteName;

    /**
     * The maximum field depth of any dynamic key mapper. Allows us to stop searching for
     * a dynamic key mapper as soon as we've passed the maximum possible field depth.
     */
    private final int maxKeyDepth;

    DynamicKeyFieldTypeLookup(Map<String, DynamicKeyFieldMapper> newMappers,
                              Map<String, String> aliasToConcreteName) {
        this.mappers = newMappers;
        this.aliasToConcreteName = aliasToConcreteName;
        this.maxKeyDepth = getMaxKeyDepth(mappers, aliasToConcreteName);
    }

    /**
     * Check if the given field corresponds to a dynamic key mapper of the
     * form 'path_to_field.path_to_key'. If so, returns a field type that
     * can be used to perform searches on this field. Otherwise returns null.
     */
    MappedFieldType get(String field) {
        if (mappers.isEmpty()) {
            return null;
        }

        int dotIndex = -1;
        int fieldDepth = 0;

        while (true) {
            if (++fieldDepth > maxKeyDepth) {
                return null;
            }

            dotIndex = field.indexOf('.', dotIndex + 1);
            if (dotIndex < 0) {
                return null;
            }

            String parentField = field.substring(0, dotIndex);
            String concreteField = aliasToConcreteName.getOrDefault(parentField, parentField);
            DynamicKeyFieldMapper mapper = mappers.get(concreteField);

            if (mapper != null) {
                String key = field.substring(dotIndex + 1);
                return mapper.keyedFieldType(key);
            }
        }
    }

    Stream<MappedFieldType> fieldTypes() {
        return mappers.values().stream().map(mapper -> mapper.keyedFieldType(""));
    }

    // Visible for testing.
    static int getMaxKeyDepth(Map<String, DynamicKeyFieldMapper> dynamicKeyMappers,
                              Map<String, String> aliasToConcreteName) {
        int maxFieldDepth = 0;
        for (Map.Entry<String, String> entry : aliasToConcreteName.entrySet()) {
            String aliasName = entry.getKey();
            String path = entry.getValue();
            if (dynamicKeyMappers.containsKey(path)) {
                maxFieldDepth = Math.max(maxFieldDepth, fieldDepth(aliasName));
            }
        }

        for (String fieldName : dynamicKeyMappers.keySet()) {
            if (dynamicKeyMappers.containsKey(fieldName)) {
                maxFieldDepth = Math.max(maxFieldDepth, fieldDepth(fieldName));
            }
        }

        return maxFieldDepth;
    }

    /**
     * Computes the total depth of this field by counting the number of parent fields
     * in its path. As an example, the field 'parent1.parent2.field' has depth 3.
     */
    private static int fieldDepth(String field) {
        int numDots = 0;
        int dotIndex = -1;
        while (true) {
            dotIndex = field.indexOf('.', dotIndex + 1);
            if (dotIndex < 0) {
                break;
            }
            numDots++;
        }
        return numDots + 1;
    }
}
