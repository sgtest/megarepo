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
package org.elasticsearch.script;

import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.admin.cluster.storedscripts.GetStoredScriptRequest;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.env.Environment;
import org.elasticsearch.test.ESTestCase;
import org.junit.Before;

import java.io.IOException;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.function.Function;

import static org.elasticsearch.script.ScriptService.MAX_COMPILATION_RATE_FUNCTION;
import static org.elasticsearch.script.ScriptService.SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING;
import static org.elasticsearch.script.ScriptService.SCRIPT_MAX_COMPILATIONS_RATE_SETTING;
import static org.hamcrest.CoreMatchers.containsString;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.sameInstance;

public class ScriptServiceTests extends ESTestCase {

    private ScriptEngine scriptEngine;
    private Map<String, ScriptEngine> engines;
    private Map<String, ScriptContext<?>> contexts;
    private ScriptService scriptService;
    private Settings baseSettings;
    private ClusterSettings clusterSettings;

    @Before
    public void setup() throws IOException {
        baseSettings = Settings.builder()
                .put(Environment.PATH_HOME_SETTING.getKey(), createTempDir().toString())
                .build();
        Map<String, Function<Map<String, Object>, Object>> scripts = new HashMap<>();
        for (int i = 0; i < 20; ++i) {
            scripts.put(i + "+" + i, p -> null); // only care about compilation, not execution
        }
        scripts.put("script", p -> null);
        scriptEngine = new MockScriptEngine(Script.DEFAULT_SCRIPT_LANG, scripts, Collections.emptyMap());
        //prevent duplicates using map
        contexts = new HashMap<>(ScriptModule.CORE_CONTEXTS);
        engines = new HashMap<>();
        engines.put(scriptEngine.getType(), scriptEngine);
        engines.put("test", new MockScriptEngine("test", scripts, Collections.emptyMap()));
        logger.info("--> setup script service");
    }

    private void buildScriptService(Settings additionalSettings) throws IOException {
        Settings finalSettings = Settings.builder().put(baseSettings).put(additionalSettings).build();
        scriptService = new ScriptService(finalSettings, engines, contexts) {
            @Override
            Map<String, StoredScriptSource> getScriptsFromClusterState() {
                Map<String, StoredScriptSource> scripts = new HashMap<>();
                scripts.put("test1", new StoredScriptSource("test", "1+1", Collections.emptyMap()));
                scripts.put("test2", new StoredScriptSource("test", "1", Collections.emptyMap()));
                return scripts;
            }

            @Override
            StoredScriptSource getScriptFromClusterState(String id) {
                //mock the script that gets retrieved from an index
                return new StoredScriptSource("test", "1+1", Collections.emptyMap());
            }
        };
        clusterSettings = new ClusterSettings(finalSettings, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS);
        scriptService.registerClusterSettingsListeners(clusterSettings);
    }

    public void testMaxCompilationRateSetting() throws Exception {
        assertThat(MAX_COMPILATION_RATE_FUNCTION.apply("10/1m"), is(Tuple.tuple(10, TimeValue.timeValueMinutes(1))));
        assertThat(MAX_COMPILATION_RATE_FUNCTION.apply("10/60s"), is(Tuple.tuple(10, TimeValue.timeValueMinutes(1))));
        assertException("10/m", IllegalArgumentException.class, "failed to parse [m]");
        assertException("6/1.6m", IllegalArgumentException.class, "failed to parse [1.6m], fractional time values are not supported");
        assertException("foo/bar", IllegalArgumentException.class, "could not parse [foo] as integer in value [foo/bar]");
        assertException("6.0/1m", IllegalArgumentException.class, "could not parse [6.0] as integer in value [6.0/1m]");
        assertException("6/-1m", IllegalArgumentException.class, "time value [-1m] must be positive");
        assertException("6/0m", IllegalArgumentException.class, "time value [0m] must be positive");
        assertException("10", IllegalArgumentException.class,
                "parameter must contain a positive integer and a timevalue, i.e. 10/1m, but was [10]");
        assertException("anything", IllegalArgumentException.class,
                "parameter must contain a positive integer and a timevalue, i.e. 10/1m, but was [anything]");
        assertException("/1m", IllegalArgumentException.class,
                "parameter must contain a positive integer and a timevalue, i.e. 10/1m, but was [/1m]");
        assertException("10/", IllegalArgumentException.class,
                "parameter must contain a positive integer and a timevalue, i.e. 10/1m, but was [10/]");
        assertException("-1/1m", IllegalArgumentException.class, "rate [-1] must be positive");
        assertException("10/5s", IllegalArgumentException.class, "time value [5s] must be at least on a one minute resolution");
    }

    private void assertException(String rate, Class<? extends Exception> clazz, String message) {
        Exception e = expectThrows(clazz, () -> MAX_COMPILATION_RATE_FUNCTION.apply(rate));
        assertThat(e.getMessage(), is(message));
    }

    public void testNotSupportedDisableDynamicSetting() throws IOException {
        try {
            buildScriptService(Settings.builder().put(
                    ScriptService.DISABLE_DYNAMIC_SCRIPTING_SETTING, randomUnicodeOfLength(randomIntBetween(1, 10))).build());
            fail("script service should have thrown exception due to non supported script.disable_dynamic setting");
        } catch(IllegalArgumentException e) {
            assertThat(e.getMessage(), containsString(ScriptService.DISABLE_DYNAMIC_SCRIPTING_SETTING +
                    " is not a supported setting, replace with fine-grained script settings"));
        }
    }

    public void testInlineScriptCompiledOnceCache() throws IOException {
        buildScriptService(Settings.EMPTY);
        Script script = new Script(ScriptType.INLINE, "test", "1+1", Collections.emptyMap());
        FieldScript.Factory factoryScript1 = scriptService.compile(script, FieldScript.CONTEXT);
        FieldScript.Factory factoryScript2 = scriptService.compile(script, FieldScript.CONTEXT);
        assertThat(factoryScript1, sameInstance(factoryScript2));
    }

    public void testAllowAllScriptTypeSettings() throws IOException {
        buildScriptService(Settings.EMPTY);

        assertCompileAccepted("painless", "script", ScriptType.INLINE, FieldScript.CONTEXT);
        assertCompileAccepted(null, "script", ScriptType.STORED, FieldScript.CONTEXT);
    }

    public void testAllowAllScriptContextSettings() throws IOException {
        buildScriptService(Settings.EMPTY);

        assertCompileAccepted("painless", "script", ScriptType.INLINE, FieldScript.CONTEXT);
        assertCompileAccepted("painless", "script", ScriptType.INLINE, AggregationScript.CONTEXT);
        assertCompileAccepted("painless", "script", ScriptType.INLINE, UpdateScript.CONTEXT);
        assertCompileAccepted("painless", "script", ScriptType.INLINE, IngestScript.CONTEXT);
    }

    public void testAllowSomeScriptTypeSettings() throws IOException {
        Settings.Builder builder = Settings.builder();
        builder.put("script.allowed_types", "inline");
        buildScriptService(builder.build());

        assertCompileAccepted("painless", "script", ScriptType.INLINE, FieldScript.CONTEXT);
        assertCompileRejected(null, "script", ScriptType.STORED, FieldScript.CONTEXT);
    }

    public void testAllowSomeScriptContextSettings() throws IOException {
        Settings.Builder builder = Settings.builder();
        builder.put("script.allowed_contexts", "field, aggs");
        buildScriptService(builder.build());

        assertCompileAccepted("painless", "script", ScriptType.INLINE, FieldScript.CONTEXT);
        assertCompileAccepted("painless", "script", ScriptType.INLINE, AggregationScript.CONTEXT);
        assertCompileRejected("painless", "script", ScriptType.INLINE, UpdateScript.CONTEXT);
    }

    public void testAllowNoScriptTypeSettings() throws IOException {
        Settings.Builder builder = Settings.builder();
        builder.put("script.allowed_types", "none");
        buildScriptService(builder.build());

        assertCompileRejected("painless", "script", ScriptType.INLINE, FieldScript.CONTEXT);
        assertCompileRejected(null, "script", ScriptType.STORED, FieldScript.CONTEXT);
    }

    public void testAllowNoScriptContextSettings() throws IOException {
        Settings.Builder builder = Settings.builder();
        builder.put("script.allowed_contexts", "none");
        buildScriptService(builder.build());

        assertCompileRejected("painless", "script", ScriptType.INLINE, FieldScript.CONTEXT);
        assertCompileRejected("painless", "script", ScriptType.INLINE, AggregationScript.CONTEXT);
    }

    public void testCompileNonRegisteredContext() throws IOException {
        contexts.remove(IngestScript.CONTEXT.name);
        buildScriptService(Settings.EMPTY);

        String type = scriptEngine.getType();
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () ->
            scriptService.compile(new Script(ScriptType.INLINE, type, "test", Collections.emptyMap()), IngestScript.CONTEXT));
        assertThat(e.getMessage(), containsString("script context [" + IngestScript.CONTEXT.name + "] not supported"));
    }

    public void testCompileCountedInCompilationStats() throws IOException {
        buildScriptService(Settings.EMPTY);
        scriptService.compile(new Script(ScriptType.INLINE, "test", "1+1", Collections.emptyMap()), randomFrom(contexts.values()));
        assertEquals(1L, scriptService.stats().getCompilations());
    }

    public void testMultipleCompilationsCountedInCompilationStats() throws IOException {
        buildScriptService(Settings.EMPTY);
        int numberOfCompilations = randomIntBetween(1, 20);
        for (int i = 0; i < numberOfCompilations; i++) {
            scriptService
                    .compile(new Script(ScriptType.INLINE, "test", i + "+" + i, Collections.emptyMap()), randomFrom(contexts.values()));
        }
        assertEquals(numberOfCompilations, scriptService.stats().getCompilations());
    }

    public void testCompilationStatsOnCacheHit() throws IOException {
        Settings.Builder builder = Settings.builder();
        builder.put(ScriptService.SCRIPT_GENERAL_CACHE_SIZE_SETTING.getKey(), 1);
        buildScriptService(builder.build());
        Script script = new Script(ScriptType.INLINE, "test", "1+1", Collections.emptyMap());
        ScriptContext<?> context = randomFrom(contexts.values());
        scriptService.compile(script, context);
        scriptService.compile(script, context);
        assertEquals(1L, scriptService.stats().getCompilations());
    }

    public void testIndexedScriptCountedInCompilationStats() throws IOException {
        buildScriptService(Settings.EMPTY);
        scriptService.compile(new Script(ScriptType.STORED, null, "script", Collections.emptyMap()), randomFrom(contexts.values()));
        assertEquals(1L, scriptService.stats().getCompilations());
    }

    public void testCacheEvictionCountedInCacheEvictionsStats() throws IOException {
        Settings.Builder builder = Settings.builder();
        builder.put(ScriptService.SCRIPT_GENERAL_CACHE_SIZE_SETTING.getKey(), 1);
        buildScriptService(builder.build());
        scriptService.compile(new Script(ScriptType.INLINE, "test", "1+1", Collections.emptyMap()), randomFrom(contexts.values()));
        scriptService.compile(new Script(ScriptType.INLINE, "test", "2+2", Collections.emptyMap()), randomFrom(contexts.values()));
        assertEquals(2L, scriptService.stats().getCompilations());
        assertEquals(1L, scriptService.stats().getCacheEvictions());
    }

    public void testStoreScript() throws Exception {
        BytesReference script = BytesReference.bytes(XContentFactory.jsonBuilder()
            .startObject()
            .field("script")
            .startObject()
            .field("lang", "_lang")
            .field("source", "abc")
            .endObject()
            .endObject());
        ScriptMetaData scriptMetaData = ScriptMetaData.putStoredScript(null, "_id", StoredScriptSource.parse(script, XContentType.JSON));
        assertNotNull(scriptMetaData);
        assertEquals("abc", scriptMetaData.getStoredScript("_id").getSource());
    }

    public void testDeleteScript() throws Exception {
        ScriptMetaData scriptMetaData = ScriptMetaData.putStoredScript(null, "_id",
            StoredScriptSource.parse(new BytesArray("{\"script\": {\"lang\": \"_lang\", \"source\": \"abc\"} }"), XContentType.JSON));
        scriptMetaData = ScriptMetaData.deleteStoredScript(scriptMetaData, "_id");
        assertNotNull(scriptMetaData);
        assertNull(scriptMetaData.getStoredScript("_id"));

        ScriptMetaData errorMetaData = scriptMetaData;
        ResourceNotFoundException e = expectThrows(ResourceNotFoundException.class, () -> {
            ScriptMetaData.deleteStoredScript(errorMetaData, "_id");
        });
        assertEquals("stored script [_id] does not exist and cannot be deleted", e.getMessage());
    }

    public void testGetStoredScript() throws Exception {
        buildScriptService(Settings.EMPTY);
        ClusterState cs = ClusterState.builder(new ClusterName("_name"))
            .metaData(MetaData.builder()
                .putCustom(ScriptMetaData.TYPE,
                    new ScriptMetaData.Builder(null).storeScript("_id",
                        StoredScriptSource.parse(new BytesArray("{\"script\": {\"lang\": \"_lang\", \"source\": \"abc\"} }"),
                            XContentType.JSON)).build()))
            .build();

        assertEquals("abc", scriptService.getStoredScript(cs, new GetStoredScriptRequest("_id")).getSource());

        cs = ClusterState.builder(new ClusterName("_name")).build();
        assertNull(scriptService.getStoredScript(cs, new GetStoredScriptRequest("_id")));
    }

    public void testMaxSizeLimit() throws Exception {
        buildScriptService(Settings.builder().put(ScriptService.SCRIPT_MAX_SIZE_IN_BYTES.getKey(), 4).build());
        scriptService.compile(new Script(ScriptType.INLINE, "test", "1+1", Collections.emptyMap()), randomFrom(contexts.values()));
        IllegalArgumentException iae = expectThrows(IllegalArgumentException.class, () -> {
            scriptService.compile(new Script(ScriptType.INLINE, "test", "10+10", Collections.emptyMap()), randomFrom(contexts.values()));
        });
        assertEquals("exceeded max allowed inline script size in bytes [4] with size [5] for script [10+10]", iae.getMessage());
        clusterSettings.applySettings(Settings.builder().put(ScriptService.SCRIPT_MAX_SIZE_IN_BYTES.getKey(), 6).build());
        scriptService.compile(new Script(ScriptType.INLINE, "test", "10+10", Collections.emptyMap()), randomFrom(contexts.values()));
        clusterSettings.applySettings(Settings.builder().put(ScriptService.SCRIPT_MAX_SIZE_IN_BYTES.getKey(), 5).build());
        scriptService.compile(new Script(ScriptType.INLINE, "test", "10+10", Collections.emptyMap()), randomFrom(contexts.values()));
        iae = expectThrows(IllegalArgumentException.class, () -> {
            clusterSettings.applySettings(Settings.builder().put(ScriptService.SCRIPT_MAX_SIZE_IN_BYTES.getKey(), 2).build());
        });
        assertEquals("script.max_size_in_bytes cannot be set to [2], stored script [test1] exceeds the new value with a size of [3]",
                iae.getMessage());
    }

    public void testConflictContextSettings() throws IOException {
        IllegalArgumentException illegal = expectThrows(IllegalArgumentException.class, () -> {
            buildScriptService(Settings.builder()
                .put(ScriptService.SCRIPT_CACHE_SIZE_SETTING.getConcreteSettingForNamespace("field").getKey(), 123).build());
        });
        assertEquals("Context cache settings [script.context.field.cache_max_size] requires " +
                     "[script.max_compilations_rate] to be [use-context]",
            illegal.getMessage()
        );

        illegal = expectThrows(IllegalArgumentException.class, () -> {
            buildScriptService(Settings.builder()
                .put(ScriptService.SCRIPT_CACHE_EXPIRE_SETTING.getConcreteSettingForNamespace("ingest").getKey(), "5m").build());
        });

        assertEquals("Context cache settings [script.context.ingest.cache_expire] requires " +
                     "[script.max_compilations_rate] to be [use-context]",
            illegal.getMessage()
        );

        illegal = expectThrows(IllegalArgumentException.class, () -> {
            buildScriptService(Settings.builder()
                .put(ScriptService.SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getConcreteSettingForNamespace("score").getKey(), "50/5m").build());
        });

        assertEquals("Context cache settings [script.context.score.max_compilations_rate] requires " +
                     "[script.max_compilations_rate] to be [use-context]",
            illegal.getMessage()
        );

        buildScriptService(
            Settings.builder()
                .put(ScriptService.SCRIPT_CACHE_EXPIRE_SETTING.getConcreteSettingForNamespace("ingest").getKey(), "5m")
                .put(ScriptService.SCRIPT_CACHE_SIZE_SETTING.getConcreteSettingForNamespace("field").getKey(), 123)
                .put(ScriptService.SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getConcreteSettingForNamespace("score").getKey(), "50/5m")
                .put(ScriptService.SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), ScriptService.USE_CONTEXT_RATE_KEY).build());
    }

    public void testFallbackContextSettings() {
        int cacheSizeBackup = randomIntBetween(0, 1024);
        int cacheSizeFoo = randomValueOtherThan(cacheSizeBackup, () -> randomIntBetween(0, 1024));

        String cacheExpireBackup = randomTimeValue(1, 1000, "h");
        TimeValue cacheExpireBackupParsed = TimeValue.parseTimeValue(cacheExpireBackup, "");
        String cacheExpireFoo = randomValueOtherThan(cacheExpireBackup, () -> randomTimeValue(1, 1000, "h"));
        TimeValue cacheExpireFooParsed = TimeValue.parseTimeValue(cacheExpireFoo, "");

        Settings s = Settings.builder()
            .put(ScriptService.SCRIPT_GENERAL_CACHE_SIZE_SETTING.getKey(), cacheSizeBackup)
            .put(ScriptService.SCRIPT_CACHE_SIZE_SETTING.getConcreteSettingForNamespace("foo").getKey(), cacheSizeFoo)
            .put(ScriptService.SCRIPT_GENERAL_CACHE_EXPIRE_SETTING.getKey(), cacheExpireBackup)
            .put(ScriptService.SCRIPT_CACHE_EXPIRE_SETTING.getConcreteSettingForNamespace("foo").getKey(), cacheExpireFoo)
            .build();

        assertEquals(cacheSizeFoo, ScriptService.SCRIPT_CACHE_SIZE_SETTING.getConcreteSettingForNamespace("foo").get(s).intValue());
        assertEquals(cacheSizeBackup, ScriptService.SCRIPT_CACHE_SIZE_SETTING.getConcreteSettingForNamespace("bar").get(s).intValue());

        assertEquals(cacheExpireFooParsed, ScriptService.SCRIPT_CACHE_EXPIRE_SETTING.getConcreteSettingForNamespace("foo").get(s));
        assertEquals(cacheExpireBackupParsed, ScriptService.SCRIPT_CACHE_EXPIRE_SETTING.getConcreteSettingForNamespace("bar").get(s));
    }

    public void testUseContextSettingValue() {
        Settings s = Settings.builder()
            .put(ScriptService.SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), ScriptService.USE_CONTEXT_RATE_KEY)
            .put(ScriptService.SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getConcreteSettingForNamespace("foo").getKey(),
                 ScriptService.USE_CONTEXT_RATE_KEY)
            .build();

        assertEquals(ScriptService.SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.get(s), ScriptService.USE_CONTEXT_RATE_VALUE);

        IllegalArgumentException illegal = expectThrows(IllegalArgumentException.class, () -> {
            ScriptService.SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getAsMap(s);
        });

        assertEquals("parameter must contain a positive integer and a timevalue, i.e. 10/1m, but was [use-context]", illegal.getMessage());
    }

    public void testCacheHolderGeneralConstructor() {
        String compilationRate = "77/5m";
        Tuple<Integer, TimeValue> rate = ScriptService.MAX_COMPILATION_RATE_FUNCTION.apply(compilationRate);
        boolean compilationLimitsEnabled = true;
        ScriptService.CacheHolder holder = new ScriptService.CacheHolder(
            Settings.builder().put(SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), compilationRate).build(),
            new HashSet<>(Collections.singleton("foo")),
            compilationLimitsEnabled
        );

        assertNotNull(holder.general);
        assertNull(holder.contextCache);
        assertEquals(holder.general.rate, rate);

        compilationLimitsEnabled = false;
        holder = new ScriptService.CacheHolder(
            Settings.builder().put(SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), compilationRate).build(),
            new HashSet<>(Collections.singleton("foo")),
            compilationLimitsEnabled
        );

        assertNotNull(holder.general);
        assertNull(holder.contextCache);
        assertEquals(holder.general.rate, new Tuple<>(0, TimeValue.ZERO));
    }

    public void testCacheHolderContextConstructor() {
        String fooCompilationRate = "77/5m";
        String barCompilationRate = "78/6m";
        boolean compilationLimitsEnabled = true;

        Settings s = Settings.builder()
            .put(SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), ScriptService.USE_CONTEXT_RATE_KEY)
            .put(SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getConcreteSettingForNamespace("foo").getKey(), fooCompilationRate)
            .put(SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getConcreteSettingForNamespace("bar").getKey(), barCompilationRate)
            .build();
        Set<String> contexts = Set.of("foo", "bar", "baz");
        ScriptService.CacheHolder holder = new ScriptService.CacheHolder(s, contexts, compilationLimitsEnabled);

        assertNull(holder.general);
        assertNotNull(holder.contextCache);
        assertEquals(3, holder.contextCache.size());
        assertEquals(contexts, holder.contextCache.keySet());

        assertEquals(ScriptService.MAX_COMPILATION_RATE_FUNCTION.apply(fooCompilationRate), holder.contextCache.get("foo").get().rate);
        assertEquals(ScriptService.MAX_COMPILATION_RATE_FUNCTION.apply(barCompilationRate), holder.contextCache.get("bar").get().rate);
        assertEquals(ScriptService.SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getDefault(Settings.EMPTY),
                     holder.contextCache.get("baz").get().rate);

        Tuple<Integer, TimeValue> zero = new Tuple<>(0, TimeValue.ZERO);
        compilationLimitsEnabled = false;
        holder = new ScriptService.CacheHolder(s, contexts, compilationLimitsEnabled);

        assertNotNull(holder.contextCache);
        assertEquals(3, holder.contextCache.size());
        assertEquals(zero, holder.contextCache.get("foo").get().rate);
        assertEquals(zero, holder.contextCache.get("bar").get().rate);
        assertEquals(zero, holder.contextCache.get("baz").get().rate);
    }

    public void testCacheHolderChangeSettings() {
        String fooCompilationRate = "77/5m";
        String barCompilationRate = "78/6m";
        String compilationRate = "77/5m";
        Tuple<Integer, TimeValue> generalRate = MAX_COMPILATION_RATE_FUNCTION.apply(compilationRate);

        Settings s = Settings.builder()
            .put(SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), compilationRate)
            .build();
        Set<String> contexts = Set.of("foo", "bar", "baz");
        ScriptService.CacheHolder holder = new ScriptService.CacheHolder(s, contexts, true);

        assertNotNull(holder.general);
        assertNull(holder.contextCache);
        assertEquals(generalRate, holder.general.rate);

        holder = holder.withUpdatedCacheSettings(Settings.builder()
            .put(SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), ScriptService.USE_CONTEXT_RATE_KEY)
            .put(SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getConcreteSettingForNamespace("foo").getKey(), fooCompilationRate)
            .put(SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getConcreteSettingForNamespace("bar").getKey(), barCompilationRate)
            .build()
        );

        assertNull(holder.general);
        assertNotNull(holder.contextCache);
        assertEquals(3, holder.contextCache.size());
        assertEquals(contexts, holder.contextCache.keySet());

        assertEquals(ScriptService.MAX_COMPILATION_RATE_FUNCTION.apply(fooCompilationRate), holder.contextCache.get("foo").get().rate);
        assertEquals(ScriptService.MAX_COMPILATION_RATE_FUNCTION.apply(barCompilationRate), holder.contextCache.get("bar").get().rate);
        assertEquals(ScriptService.SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getDefault(Settings.EMPTY),
            holder.contextCache.get("baz").get().rate);

        holder.updateContextSettings(Settings.builder()
                .put(SCRIPT_MAX_COMPILATIONS_RATE_SETTING.getConcreteSettingForNamespace("bar").getKey(), fooCompilationRate).build(),
                "bar"
        );
        assertEquals(ScriptService.MAX_COMPILATION_RATE_FUNCTION.apply(fooCompilationRate), holder.contextCache.get("bar").get().rate);

        holder = holder.withUpdatedCacheSettings(s);
        assertNotNull(holder.general);
        assertNull(holder.contextCache);
        assertEquals(generalRate, holder.general.rate);

        holder = holder.withUpdatedCacheSettings(
            Settings.builder().put(SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), barCompilationRate).build()
        );

        assertNotNull(holder.general);
        assertNull(holder.contextCache);
        assertEquals(MAX_COMPILATION_RATE_FUNCTION.apply(barCompilationRate), holder.general.rate);

        ScriptService.CacheHolder update = holder.withUpdatedCacheSettings(
            Settings.builder().put(SCRIPT_GENERAL_MAX_COMPILATIONS_RATE_SETTING.getKey(), barCompilationRate).build()
        );
        assertSame(holder, update);
    }

    private void assertCompileRejected(String lang, String script, ScriptType scriptType, ScriptContext scriptContext) {
        try {
            scriptService.compile(new Script(scriptType, lang, script, Collections.emptyMap()), scriptContext);
            fail("compile should have been rejected for lang [" + lang + "], " +
                    "script_type [" + scriptType + "], scripted_op [" + scriptContext + "]");
        } catch (IllegalArgumentException | IllegalStateException e) {
            // pass
        }
    }

    private void assertCompileAccepted(String lang, String script, ScriptType scriptType, ScriptContext scriptContext) {
        assertThat(
                scriptService.compile(new Script(scriptType, lang, script, Collections.emptyMap()), scriptContext),
                notNullValue()
        );
    }
}
