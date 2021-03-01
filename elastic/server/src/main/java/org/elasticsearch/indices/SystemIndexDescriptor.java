/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.indices;

import org.apache.lucene.util.automaton.Automaton;
import org.apache.lucene.util.automaton.CharacterRunAutomaton;
import org.apache.lucene.util.automaton.Operations;
import org.apache.lucene.util.automaton.RegExp;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.json.JsonXContent;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;

/**
 * A system index descriptor describes one or more system indices. It can match a number of indices using
 * a pattern. For system indices that are managed externally to Elasticsearch, this is enough. For system
 * indices that are managed internally to Elasticsearch, a descriptor can also include information for
 * creating the system index, upgrading its mappings, and creating an alias.
 */
public class SystemIndexDescriptor {
    /** A pattern, either with a wildcard or simple regex. Indices that match one of these patterns are considered system indices. */
    private final String indexPattern;

    /**
     * For internally-managed indices, specifies the name of the concrete index to create and update. This is required
     * since the {@link #indexPattern} can match many indices.
     */
    private final String primaryIndex;

    /** A description of the index or indices */
    private final String description;

    /** Used to determine whether an index name matches the {@link #indexPattern} */
    private final CharacterRunAutomaton indexPatternAutomaton;

    /** For internally-managed indices, contains the index mappings JSON */
    private final String mappings;

    /** For internally-managed indices, contains the index settings */
    private final Settings settings;

    /** For internally-managed indices, an optional alias to create */
    private final String aliasName;

    /** For internally-managed indices, an optional {@link IndexMetadata#INDEX_FORMAT_SETTING} value to expect */
    private final int indexFormat;

    /**
     * For internally-managed indices, specifies a key name under <code>_meta</code> in the index mappings
     * that contains the index's mappings' version.
     */
    private final String versionMetaKey;

    /** For internally-managed indices, specifies the origin to use when creating or updating the index */
    private final String origin;

    /** The minimum cluster node version required for this descriptor, or null if there is no restriction */
    private final Version minimumNodeVersion;

    /** Whether there are dynamic fields in this descriptor's mappings */
    private final boolean hasDynamicMappings;

    /** The {@link Type} of system index this descriptor represents */
    private final Type type;

    /** A list of allowed product origins that may access an external system index */
    private final List<String> allowedElasticProductOrigins;

    /**
     * Creates a descriptor for system indices matching the supplied pattern. These indices will not be managed
     * by Elasticsearch internally.
     * @param indexPattern The pattern of index names that this descriptor will be used for. Must start with a '.' character.
     * @param description The name of the plugin responsible for this system index.
     */
    public SystemIndexDescriptor(String indexPattern, String description) {
        this(indexPattern, null, description, null, null, null, 0, null, null, null, Type.INTERNAL, List.of());
    }

    /**
     * Creates a descriptor for system indices matching the supplied pattern. These indices will not be managed
     * by Elasticsearch internally.
     * @param indexPattern The pattern of index names that this descriptor will be used for. Must start with a '.' character.
     * @param description The name of the plugin responsible for this system index.
     * @param type The {@link Type} of system index
     * @param allowedElasticProductOrigins A list of allowed origin values that should be allowed access in the case of external system
     *                                     indices
     */
    public SystemIndexDescriptor(String indexPattern, String description, Type type, List<String> allowedElasticProductOrigins) {
        this(indexPattern, null, description, null, null, null, 0, null, null, null, type, allowedElasticProductOrigins);
    }

    /**
     * Creates a descriptor for system indices matching the supplied pattern. These indices will be managed
     * by Elasticsearch internally if mappings or settings are provided.
     *
     * @param indexPattern The pattern of index names that this descriptor will be used for. Must start with a '.' character.
     * @param description The name of the plugin responsible for this system index.
     * @param mappings The mappings to apply to this index when auto-creating, if appropriate
     * @param settings The settings to apply to this index when auto-creating, if appropriate
     * @param aliasName An alias for the index, or null
     * @param indexFormat A value for the `index.format` setting. Pass 0 or higher.
     * @param versionMetaKey a mapping key under <code>_meta</code> where a version can be found, which indicates the
    *                       Elasticsearch version when the index was created.
     * @param origin the client origin to use when creating this index.
     * @param minimumNodeVersion the minimum cluster node version required for this descriptor, or null if there is no restriction
     * @param type The {@link Type} of system index
     * @param allowedElasticProductOrigins A list of allowed origin values that should be allowed access in the case of external system
     *                                     indices
     */
    SystemIndexDescriptor(
        String indexPattern,
        String primaryIndex,
        String description,
        String mappings,
        Settings settings,
        String aliasName,
        int indexFormat,
        String versionMetaKey,
        String origin,
        Version minimumNodeVersion,
        Type type,
        List<String> allowedElasticProductOrigins
    ) {
        Objects.requireNonNull(indexPattern, "system index pattern must not be null");
        if (indexPattern.length() < 2) {
            throw new IllegalArgumentException(
                "system index pattern provided as [" + indexPattern + "] but must at least 2 characters in length"
            );
        }
        if (indexPattern.charAt(0) != '.') {
            throw new IllegalArgumentException(
                "system index pattern provided as [" + indexPattern + "] but must start with the character [.]"
            );
        }
        if (indexPattern.charAt(1) == '*') {
            throw new IllegalArgumentException(
                "system index pattern provided as ["
                    + indexPattern
                    + "] but must not start with the character sequence [.*] to prevent conflicts"
            );
        }

        if (primaryIndex != null) {
            if (primaryIndex.charAt(0) != '.') {
                throw new IllegalArgumentException(
                    "system primary index provided as [" + primaryIndex + "] but must start with the character [.]"
                );
            }
            if (primaryIndex.matches("^\\.[\\w-]+$") == false) {
                throw new IllegalArgumentException(
                    "system primary index provided as [" + primaryIndex + "] but cannot contain special characters or patterns"
                );
            }
        }

        if (indexFormat < 0) {
            throw new IllegalArgumentException("Index format cannot be negative");
        }

        Strings.requireNonEmpty(indexPattern, "indexPattern must be supplied");

        if (mappings != null || settings != null) {
            Strings.requireNonEmpty(primaryIndex, "Must supply primaryIndex if mappings or settings are defined");
            Strings.requireNonEmpty(versionMetaKey, "Must supply versionMetaKey if mappings or settings are defined");
            Strings.requireNonEmpty(origin, "Must supply origin if mappings or settings are defined");
        }
        Objects.requireNonNull(type, "type must not be null");
        Objects.requireNonNull(allowedElasticProductOrigins, "allowedProductOrigins must not be null");
        if (type.isInternal() && allowedElasticProductOrigins.isEmpty() == false) {
            throw new IllegalArgumentException("Allowed origins are not valid for internal system indices");
        } else if (type.isExternal() && allowedElasticProductOrigins.isEmpty()) {
            throw new IllegalArgumentException("External system indices without allowed products is not a valid combination");
        }

        this.indexPattern = indexPattern;
        this.primaryIndex = primaryIndex;

        final Automaton automaton = buildAutomaton(indexPattern, aliasName);
        this.indexPatternAutomaton = new CharacterRunAutomaton(automaton);

        this.description = description;
        this.mappings = mappings;
        this.settings = settings;
        this.aliasName = aliasName;
        this.indexFormat = indexFormat;
        this.versionMetaKey = versionMetaKey;
        this.origin = origin;
        this.minimumNodeVersion = minimumNodeVersion;
        this.type = type;
        this.allowedElasticProductOrigins = allowedElasticProductOrigins;
        this.hasDynamicMappings = this.mappings != null
            && findDynamicMapping(XContentHelper.convertToMap(JsonXContent.jsonXContent, mappings, false));
    }


    /**
     * @return The pattern of index names that this descriptor will be used for.
     */
    public String getIndexPattern() {
        return indexPattern;
    }

    /**
     * @return The concrete name of an index being managed internally to Elasticsearch. Will be {@code null}
     * for indices managed externally to Elasticsearch.
     */
    public String getPrimaryIndex() {
        return primaryIndex;
    }

    /**
     * Checks whether an index name matches the system index name pattern for this descriptor.
     * @param index The index name to be checked against the index pattern given at construction time.
     * @return True if the name matches the pattern, false otherwise.
     */
    public boolean matchesIndexPattern(String index) {
        return indexPatternAutomaton.run(index);
    }

    /**
     * Retrieves a list of all indices which match this descriptor's pattern.
     *
     * This cannot be done via {@link org.elasticsearch.cluster.metadata.IndexNameExpressionResolver} because that class can only handle
     * simple wildcard expressions, but system index name patterns may use full Lucene regular expression syntax,
     *
     * @param metadata The current metadata to get the list of matching indices from
     * @return A list of index names that match this descriptor
     */
    public List<String> getMatchingIndices(Metadata metadata) {
        ArrayList<String> matchingIndices = new ArrayList<>();
        metadata.indices().keysIt().forEachRemaining(indexName -> {
            if (matchesIndexPattern(indexName)) {
                matchingIndices.add(indexName);
            }
        });

        return Collections.unmodifiableList(matchingIndices);
    }

    /**
     * @return A short description of the purpose of this system index.
     */
    public String getDescription() {
        return description;
    }

    @Override
    public String toString() {
        return "SystemIndexDescriptor[pattern=[" + indexPattern + "], description=[" + description + "], aliasName=[" + aliasName + "]]";
    }

    public String getMappings() {
        return mappings;
    }

    public Settings getSettings() {
        return settings;
    }

    public String getAliasName() {
        return aliasName;
    }

    public int getIndexFormat() {
        return this.indexFormat;
    }

    public String getVersionMetaKey() {
        return this.versionMetaKey;
    }

    public boolean isAutomaticallyManaged() {
        // TODO remove mappings/settings check after all internal indices have been migrated
        return type.isManaged() && (this.mappings != null || this.settings != null);
    }

    public String getOrigin() {
        return this.origin;
    }

    public boolean hasDynamicMappings() {
        return this.hasDynamicMappings;
    }

    public boolean isExternal() {
        return type.isExternal();
    }

    public boolean isInternal() {
        return type.isInternal();
    }

    public List<String> getAllowedElasticProductOrigins() {
        return allowedElasticProductOrigins;
    }

    /**
     * Checks that this descriptor can be used within this cluster, by comparing the supplied minimum
     * node version to this descriptor's minimum version.
     *
     * @param cause the action being attempted that triggered the check. Used in the error message.
     * @param actualMinimumNodeVersion the lower node version in the cluster
     * @return an error message if the lowest node version is lower that the version in this descriptor,
     * or <code>null</code> if the supplied version is acceptable or this descriptor has no minimum version.
     */
    public String checkMinimumNodeVersion(String cause, Version actualMinimumNodeVersion) {
        Objects.requireNonNull(cause);
        if (this.minimumNodeVersion != null && this.minimumNodeVersion.after(actualMinimumNodeVersion)) {
            return String.format(
                Locale.ROOT,
                "[%s] failed - system index [%s] requires all cluster nodes to be at least version [%s]",
                cause,
                this.getPrimaryIndex(),
                minimumNodeVersion
            );
        }
        return null;
    }

    public static Builder builder() {
        return new Builder();
    }

    /**
     * The specific type of system index that this descriptor represents. System indices have three defined types, which is used to
     * control behavior. Elasticsearch itself and plugins have system indices that are necessary for their features;
     * these system indices are referred to as internal system indices. Internal system indices are always managed indices that
     * Elasticsearch manages.
     *
     * System indices can also belong to features outside of Elasticsearch that may be part of other Elastic stack components. These are
     * external system indices as the intent is for these to be accessed via normal APIs with a special value. Within external system
     * indices, there are two sub-types. The first are those that are managed by Elasticsearch and will have mappings/settings changed as
     * the cluster itself is upgraded. The second are those managed by the external application and for those Elasticsearch will not
     * perform any updates.
     */
    public enum Type {
        INTERNAL(false, true),
        EXTERNAL_MANAGED(true, true),
        EXTERNAL_UNMANAGED(true, false);

        private final boolean external;
        private final boolean managed;

        Type(boolean external, boolean managed) {
            this.external = external;
            this.managed = managed;
        }

        public boolean isExternal() {
            return external;
        }

        public boolean isManaged() {
            return managed;
        }

        public boolean isInternal() {
            return external == false;
        }
    }

    /**
     * Provides a fluent API for building a {@link SystemIndexDescriptor}. Validation still happens in that class.
     */
    public static class Builder {
        private String indexPattern;
        private String primaryIndex;
        private String description;
        private String mappings = null;
        private Settings settings = null;
        private String aliasName = null;
        private int indexFormat = 0;
        private String versionMetaKey = null;
        private String origin = null;
        private Version minimumNodeVersion = null;
        private Type type = Type.INTERNAL;
        private List<String> allowedElasticProductOrigins = List.of();

        private Builder() {}

        public Builder setIndexPattern(String indexPattern) {
            this.indexPattern = indexPattern;
            return this;
        }

        public Builder setPrimaryIndex(String primaryIndex) {
            this.primaryIndex = primaryIndex;
            return this;
        }

        public Builder setDescription(String description) {
            this.description = description;
            return this;
        }

        public Builder setMappings(XContentBuilder mappingsBuilder) {
            mappings = mappingsBuilder == null ? null : Strings.toString(mappingsBuilder);
            return this;
        }

        public Builder setMappings(String mappings) {
            this.mappings = mappings;
            return this;
        }

        public Builder setSettings(Settings settings) {
            this.settings = settings;
            return this;
        }

        public Builder setAliasName(String aliasName) {
            this.aliasName = aliasName;
            return this;
        }

        public Builder setIndexFormat(int indexFormat) {
            this.indexFormat = indexFormat;
            return this;
        }

        public Builder setVersionMetaKey(String versionMetaKey) {
            this.versionMetaKey = versionMetaKey;
            return this;
        }

        public Builder setOrigin(String origin) {
            this.origin = origin;
            return this;
        }

        public Builder setMinimumNodeVersion(Version version) {
            this.minimumNodeVersion = version;
            return this;
        }

        public Builder setType(Type type) {
            this.type = type;
            return this;
        }

        public Builder setAllowedElasticProductOrigins(List<String> allowedElasticProductOrigins) {
            this.allowedElasticProductOrigins = allowedElasticProductOrigins;
            return this;
        }

        /**
         * Builds a {@link SystemIndexDescriptor} using the fields supplied to this builder.
         * @return a populated descriptor.
         */
        public SystemIndexDescriptor build() {

            return new SystemIndexDescriptor(
                indexPattern,
                primaryIndex,
                description,
                mappings,
                settings,
                aliasName,
                indexFormat,
                versionMetaKey,
                origin,
                minimumNodeVersion,
                type,
                allowedElasticProductOrigins
            );
        }
    }

    /**
     * Builds an automaton for matching index names against this descriptor's index pattern.
     * If this descriptor has an alias name, the automaton will also try to match against
     * the alias as well.
     */
    static Automaton buildAutomaton(String pattern, String alias) {
        final String patternAsRegex = patternToRegex(pattern);
        final String aliasAsRegex = alias == null ? null : patternToRegex(alias);

        final Automaton patternAutomaton = new RegExp(patternAsRegex).toAutomaton();

        if (aliasAsRegex == null) {
            return patternAutomaton;
        }

        final Automaton aliasAutomaton = new RegExp(aliasAsRegex).toAutomaton();

        return Operations.union(patternAutomaton, aliasAutomaton);
    }

    /**
     * Translate a simple string pattern into a regular expression, suitable for creating a
     * {@link RegExp} instance. This exists because although
     * {@link org.elasticsearch.common.regex.Regex#simpleMatchToAutomaton(String)} is useful
     * for simple patterns, it doesn't support character ranges.
     *
     * @param input the string to translate
     * @return the translate string
     */
    private static String patternToRegex(String input) {
        String output = input;
        output = output.replaceAll("\\.", "\\.");
        output = output.replaceAll("\\*", ".*");
        return output;
    }

    /**
     * Recursively searches for <code>dynamic: true</code> in the supplies mappings
     * @param map a parsed fragment of an index's mappings
     * @return whether the fragment contains a dynamic mapping
     */
    @SuppressWarnings("unchecked")
    static boolean findDynamicMapping(Map<String, Object> map) {
        if (map == null) {
            return false;
        }

        for (Map.Entry<String, Object> entry : map.entrySet()) {
            final String key = entry.getKey();
            final Object value = entry.getValue();
            if (key.equals("dynamic") && (value instanceof Boolean) && ((Boolean) value)) {
                return true;
            }

            if (value instanceof Map) {
                if (findDynamicMapping((Map<String, Object>) value)) {
                    return true;
                }
            }
        }

        return false;
    }
}
