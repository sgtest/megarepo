/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.action.admin.indices.get;

import com.carrotsearch.hppc.cursors.ObjectObjectCursor;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.cluster.metadata.AliasMetadata;
import org.elasticsearch.cluster.metadata.MappingMetadata;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.mapper.MapperService;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

/**
 * A response for a get index action.
 */
public class GetIndexResponse extends ActionResponse implements ToXContentObject {

    private ImmutableOpenMap<String, MappingMetadata> mappings = ImmutableOpenMap.of();
    private ImmutableOpenMap<String, List<AliasMetadata>> aliases = ImmutableOpenMap.of();
    private ImmutableOpenMap<String, Settings> settings = ImmutableOpenMap.of();
    private ImmutableOpenMap<String, Settings> defaultSettings = ImmutableOpenMap.of();
    private ImmutableOpenMap<String, String> dataStreams = ImmutableOpenMap.of();
    private String[] indices;

    public GetIndexResponse(String[] indices,
                     ImmutableOpenMap<String, MappingMetadata> mappings,
                     ImmutableOpenMap<String, List<AliasMetadata>> aliases,
                     ImmutableOpenMap<String, Settings> settings,
                     ImmutableOpenMap<String, Settings> defaultSettings,
                     ImmutableOpenMap<String, String> dataStreams) {
        this.indices = indices;
        // to have deterministic order
        Arrays.sort(indices);
        if (mappings != null) {
            this.mappings = mappings;
        }
        if (aliases != null) {
            this.aliases = aliases;
        }
        if (settings != null) {
            this.settings = settings;
        }
        if (defaultSettings != null) {
            this.defaultSettings = defaultSettings;
        }
        if (dataStreams != null) {
            this.dataStreams = dataStreams;
        }
    }

    GetIndexResponse(StreamInput in) throws IOException {
        super(in);
        this.indices = in.readStringArray();

        int mappingsSize = in.readVInt();
        ImmutableOpenMap.Builder<String, MappingMetadata> mappingsMapBuilder = ImmutableOpenMap.builder();
        for (int i = 0; i < mappingsSize; i++) {
            String index = in.readString();
            if (in.getVersion().before(Version.V_8_0_0)) {
                int numMappings = in.readVInt();
                assert numMappings == 0 || numMappings == 1 : "Expected 0 or 1 mappings but got " + numMappings;
                if (numMappings == 1) {
                    String type = in.readString();
                    assert MapperService.SINGLE_MAPPING_NAME.equals(type) : "Expected [_doc] but got [" + type + "]";
                    mappingsMapBuilder.put(index, new MappingMetadata(in));
                }
                else {
                    mappingsMapBuilder.put(index, MappingMetadata.EMPTY_MAPPINGS);
                }
            } else {
                boolean hasMapping = in.readBoolean();
                mappingsMapBuilder.put(index, hasMapping ? new MappingMetadata(in) : MappingMetadata.EMPTY_MAPPINGS);
            }
        }
        mappings = mappingsMapBuilder.build();

        int aliasesSize = in.readVInt();
        ImmutableOpenMap.Builder<String, List<AliasMetadata>> aliasesMapBuilder = ImmutableOpenMap.builder();
        for (int i = 0; i < aliasesSize; i++) {
            String key = in.readString();
            int valueSize = in.readVInt();
            List<AliasMetadata> aliasEntryBuilder = new ArrayList<>(valueSize);
            for (int j = 0; j < valueSize; j++) {
                aliasEntryBuilder.add(new AliasMetadata(in));
            }
            aliasesMapBuilder.put(key, Collections.unmodifiableList(aliasEntryBuilder));
        }
        aliases = aliasesMapBuilder.build();

        int settingsSize = in.readVInt();
        ImmutableOpenMap.Builder<String, Settings> settingsMapBuilder = ImmutableOpenMap.builder();
        for (int i = 0; i < settingsSize; i++) {
            String key = in.readString();
            settingsMapBuilder.put(key, Settings.readSettingsFromStream(in));
        }
        settings = settingsMapBuilder.build();

        ImmutableOpenMap.Builder<String, Settings> defaultSettingsMapBuilder = ImmutableOpenMap.builder();
        int defaultSettingsSize = in.readVInt();
        for (int i = 0; i < defaultSettingsSize; i++) {
            defaultSettingsMapBuilder.put(in.readString(), Settings.readSettingsFromStream(in));
        }
        defaultSettings = defaultSettingsMapBuilder.build();

        if (in.getVersion().onOrAfter(Version.V_7_8_0)) {
            ImmutableOpenMap.Builder<String, String> dataStreamsMapBuilder = ImmutableOpenMap.builder();
            int dataStreamsSize = in.readVInt();
            for (int i = 0; i < dataStreamsSize; i++) {
                dataStreamsMapBuilder.put(in.readString(), in.readOptionalString());
            }
            dataStreams = dataStreamsMapBuilder.build();
        }
    }

    public String[] indices() {
        return indices;
    }

    public String[] getIndices() {
        return indices();
    }

    public ImmutableOpenMap<String, MappingMetadata> mappings() {
        return mappings;
    }

    public ImmutableOpenMap<String, MappingMetadata> getMappings() {
        return mappings();
    }

    public ImmutableOpenMap<String, List<AliasMetadata>> aliases() {
        return aliases;
    }

    public ImmutableOpenMap<String, List<AliasMetadata>> getAliases() {
        return aliases();
    }

    public ImmutableOpenMap<String, Settings> settings() {
        return settings;
    }

    public ImmutableOpenMap<String, String> dataStreams() {
        return dataStreams;
    }

    public ImmutableOpenMap<String, String> getDataStreams() {
        return dataStreams();
    }

    /**
     * If the originating {@link GetIndexRequest} object was configured to include
     * defaults, this will contain a mapping of index name to {@link Settings} objects.
     * The returned {@link Settings} objects will contain only those settings taking
     * effect as defaults.  Any settings explicitly set on the index will be available
     * via {@link #settings()}.
     * See also {@link GetIndexRequest#includeDefaults(boolean)}
     */
    public ImmutableOpenMap<String, Settings> defaultSettings() {
        return defaultSettings;
    }

    public ImmutableOpenMap<String, Settings> getSettings() {
        return settings();
    }

    /**
     * Returns the string value for the specified index and setting.  If the includeDefaults flag was not set or set to
     * false on the {@link GetIndexRequest}, this method will only return a value where the setting was explicitly set
     * on the index.  If the includeDefaults flag was set to true on the {@link GetIndexRequest}, this method will fall
     * back to return the default value if the setting was not explicitly set.
     */
    public String getSetting(String index, String setting) {
        Settings indexSettings = settings.get(index);
        if (setting != null) {
            if (indexSettings != null && indexSettings.hasValue(setting)) {
                return indexSettings.get(setting);
            } else {
                Settings defaultIndexSettings = defaultSettings.get(index);
                if (defaultIndexSettings != null) {
                    return defaultIndexSettings.get(setting);
                } else {
                    return null;
                }
            }
        } else {
            return null;
        }
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeStringArray(indices);
        out.writeVInt(mappings.size());
        for (ObjectObjectCursor<String, MappingMetadata> indexEntry : mappings) {
            out.writeString(indexEntry.key);
            if (out.getVersion().before(Version.V_8_0_0)) {
                out.writeVInt(indexEntry.value == MappingMetadata.EMPTY_MAPPINGS ? 0 : 1);
                if (indexEntry.value != MappingMetadata.EMPTY_MAPPINGS) {
                    out.writeString(MapperService.SINGLE_MAPPING_NAME);
                    indexEntry.value.writeTo(out);
                }
            } else {
                out.writeBoolean(indexEntry.value != MappingMetadata.EMPTY_MAPPINGS);
                if (indexEntry.value != MappingMetadata.EMPTY_MAPPINGS) {
                    indexEntry.value.writeTo(out);
                }
            }
        }
        out.writeVInt(aliases.size());
        for (ObjectObjectCursor<String, List<AliasMetadata>> indexEntry : aliases) {
            out.writeString(indexEntry.key);
            out.writeVInt(indexEntry.value.size());
            for (AliasMetadata aliasEntry : indexEntry.value) {
                aliasEntry.writeTo(out);
            }
        }
        out.writeVInt(settings.size());
        for (ObjectObjectCursor<String, Settings> indexEntry : settings) {
            out.writeString(indexEntry.key);
            Settings.writeSettingsToStream(indexEntry.value, out);
        }
        out.writeVInt(defaultSettings.size());
        for (ObjectObjectCursor<String, Settings> indexEntry : defaultSettings) {
            out.writeString(indexEntry.key);
            Settings.writeSettingsToStream(indexEntry.value, out);
        }
        if (out.getVersion().onOrAfter(Version.V_7_8_0)) {
            out.writeVInt(dataStreams.size());
            for (ObjectObjectCursor<String, String> indexEntry : dataStreams) {
                out.writeString(indexEntry.key);
                out.writeOptionalString(indexEntry.value);
            }
        }
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        {
            for (final String index : indices) {
                builder.startObject(index);
                {
                    builder.startObject("aliases");
                    List<AliasMetadata> indexAliases = aliases.get(index);
                    if (indexAliases != null) {
                        for (final AliasMetadata alias : indexAliases) {
                            AliasMetadata.Builder.toXContent(alias, builder, params);
                        }
                    }
                    builder.endObject();

                    MappingMetadata indexMappings = mappings.get(index);
                    if (indexMappings == null) {
                        builder.startObject("mappings").endObject();
                    } else {
                        builder.field("mappings", indexMappings.sourceAsMap());
                    }

                    builder.startObject("settings");
                    Settings indexSettings = settings.get(index);
                    if (indexSettings != null) {
                        indexSettings.toXContent(builder, params);
                    }
                    builder.endObject();

                    Settings defaultIndexSettings = defaultSettings.get(index);
                    if (defaultIndexSettings != null && defaultIndexSettings.isEmpty() == false) {
                        builder.startObject("defaults");
                        defaultIndexSettings.toXContent(builder, params);
                        builder.endObject();
                    }

                    String dataStream = dataStreams.get(index);
                    if (dataStream != null) {
                        builder.field("data_stream", dataStream);
                    }
                }
                builder.endObject();
            }
        }
        builder.endObject();
        return builder;
    }

    @Override
    public String toString() {
        return Strings.toString(this);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o== null || getClass() != o.getClass()) return false;
        GetIndexResponse that = (GetIndexResponse) o;
        return Arrays.equals(indices, that.indices) &&
            Objects.equals(aliases, that.aliases) &&
            Objects.equals(mappings, that.mappings) &&
            Objects.equals(settings, that.settings) &&
            Objects.equals(defaultSettings, that.defaultSettings) &&
            Objects.equals(dataStreams, that.dataStreams);
    }

    @Override
    public int hashCode() {
        return
            Objects.hash(
                Arrays.hashCode(indices),
                aliases,
                mappings,
                settings,
                defaultSettings,
                dataStreams
            );
    }
}
