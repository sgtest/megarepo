/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.plugins;

import org.elasticsearch.Version;
import org.elasticsearch.action.admin.cluster.node.info.PluginsAndModules;
import org.elasticsearch.common.io.stream.ByteBufferStreamInput;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.test.ESTestCase;

import java.nio.ByteBuffer;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;

public class PluginInfoTests extends ESTestCase {

    public void testReadFromProperties() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "classname",
            "FakePlugin",
            "modulename",
            "org.mymodule"
        );
        PluginInfo info = PluginInfo.readFromProperties(pluginDir);
        assertEquals("my_plugin", info.getName());
        assertEquals("fake desc", info.getDescription());
        assertEquals("1.0", info.getVersion());
        assertEquals("FakePlugin", info.getClassname());
        assertEquals("org.mymodule", info.getModuleName().orElseThrow());
        assertThat(info.getExtendedPlugins(), empty());
    }

    public void testReadFromPropertiesNameMissing() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(pluginDir);
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("property [name] is missing in"));

        PluginTestUtil.writePluginProperties(pluginDir, "name", "");
        e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("property [name] is missing in"));
    }

    public void testReadFromPropertiesDescriptionMissing() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(pluginDir, "name", "fake-plugin");
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("[description] is missing"));
    }

    public void testReadFromPropertiesVersionMissing() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(pluginDir, "description", "fake desc", "name", "fake-plugin");
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("[version] is missing"));
    }

    public void testReadFromPropertiesElasticsearchVersionMissing() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(pluginDir, "description", "fake desc", "name", "my_plugin", "version", "1.0");
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("[elasticsearch.version] is missing"));
    }

    public void testReadFromPropertiesElasticsearchVersionEmpty() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            "  "
        );
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("[elasticsearch.version] is missing"));
    }

    public void testReadFromPropertiesJavaVersionMissing() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "version",
            "1.0"
        );
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("[java.version] is missing"));
    }

    public void testReadFromPropertiesBadJavaVersionFormat() throws Exception {
        String pluginName = "fake-plugin";
        Path pluginDir = createTempDir().resolve(pluginName);
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            pluginName,
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            "1.7.0_80",
            "classname",
            "FakePlugin",
            "version",
            "1.0"
        );
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), equalTo("Invalid version string: '1.7.0_80'"));
    }

    public void testReadFromPropertiesBogusElasticsearchVersion() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "version",
            "1.0",
            "name",
            "my_plugin",
            "elasticsearch.version",
            "bogus"
        );
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("version needs to contain major, minor, and revision"));
    }

    public void testReadFromPropertiesJvmMissingClassname() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version")
        );
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("property [classname] is missing"));
    }

    public void testReadFromPropertiesModulenameFallback() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "classname",
            "FakePlugin"
        );
        PluginInfo info = PluginInfo.readFromProperties(pluginDir);
        assertThat(info.getModuleName().isPresent(), is(false));
        assertThat(info.getExtendedPlugins(), empty());
    }

    public void testReadFromPropertiesModulenameEmpty() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "classname",
            "FakePlugin",
            "modulename",
            " "
        );
        PluginInfo info = PluginInfo.readFromProperties(pluginDir);
        assertThat(info.getModuleName().isPresent(), is(false));
        assertThat(info.getExtendedPlugins(), empty());
    }

    public void testExtendedPluginsSingleExtension() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "classname",
            "FakePlugin",
            "extended.plugins",
            "foo"
        );
        PluginInfo info = PluginInfo.readFromProperties(pluginDir);
        assertThat(info.getExtendedPlugins(), contains("foo"));
    }

    public void testExtendedPluginsMultipleExtensions() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "classname",
            "FakePlugin",
            "extended.plugins",
            "foo,bar,baz"
        );
        PluginInfo info = PluginInfo.readFromProperties(pluginDir);
        assertThat(info.getExtendedPlugins(), contains("foo", "bar", "baz"));
    }

    public void testExtendedPluginsEmpty() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "classname",
            "FakePlugin",
            "extended.plugins",
            ""
        );
        PluginInfo info = PluginInfo.readFromProperties(pluginDir);
        assertThat(info.getExtendedPlugins(), empty());
    }

    public void testSerialize() throws Exception {
        PluginInfo info = new PluginInfo(
            "c",
            "foo",
            "dummy",
            Version.CURRENT,
            "1.8",
            "dummyclass",
            null,
            Collections.singletonList("foo"),
            randomBoolean(),
            PluginType.ISOLATED,
            "-Dfoo=bar",
            randomBoolean()
        );
        BytesStreamOutput output = new BytesStreamOutput();
        info.writeTo(output);
        ByteBuffer buffer = ByteBuffer.wrap(output.bytes().toBytesRef().bytes);
        ByteBufferStreamInput input = new ByteBufferStreamInput(buffer);
        PluginInfo info2 = new PluginInfo(input);
        assertThat(info2.toString(), equalTo(info.toString()));
    }

    public void testSerializeWithModuleName() throws Exception {
        PluginInfo info = new PluginInfo(
            "c",
            "foo",
            "dummy",
            Version.CURRENT,
            "1.8",
            "dummyclass",
            "some.module",
            Collections.singletonList("foo"),
            randomBoolean(),
            PluginType.ISOLATED,
            "-Dfoo=bar",
            randomBoolean()
        );
        BytesStreamOutput output = new BytesStreamOutput();
        info.writeTo(output);
        ByteBuffer buffer = ByteBuffer.wrap(output.bytes().toBytesRef().bytes);
        ByteBufferStreamInput input = new ByteBufferStreamInput(buffer);
        PluginInfo info2 = new PluginInfo(input);
        assertThat(info2.toString(), equalTo(info.toString()));
    }

    public void testPluginListSorted() {
        List<PluginInfo> plugins = new ArrayList<>();
        plugins.add(
            new PluginInfo(
                "c",
                "foo",
                "dummy",
                Version.CURRENT,
                "1.8",
                "dummyclass",
                null,
                Collections.emptyList(),
                randomBoolean(),
                PluginType.ISOLATED,
                "-Da",
                randomBoolean()
            )
        );
        plugins.add(
            new PluginInfo(
                "b",
                "foo",
                "dummy",
                Version.CURRENT,
                "1.8",
                "dummyclass",
                null,
                Collections.emptyList(),
                randomBoolean(),
                PluginType.BOOTSTRAP,
                "-Db",
                randomBoolean()
            )
        );
        plugins.add(
            new PluginInfo(
                "e",
                "foo",
                "dummy",
                Version.CURRENT,
                "1.8",
                "dummyclass",
                null,
                Collections.emptyList(),
                randomBoolean(),
                PluginType.ISOLATED,
                "-Dc",
                randomBoolean()
            )
        );
        plugins.add(
            new PluginInfo(
                "a",
                "foo",
                "dummy",
                Version.CURRENT,
                "1.8",
                "dummyclass",
                null,
                Collections.emptyList(),
                randomBoolean(),
                PluginType.BOOTSTRAP,
                "-Dd",
                randomBoolean()
            )
        );
        plugins.add(
            new PluginInfo(
                "d",
                "foo",
                "dummy",
                Version.CURRENT,
                "1.8",
                "dummyclass",
                null,
                Collections.emptyList(),
                randomBoolean(),
                PluginType.ISOLATED,
                "-De",
                randomBoolean()
            )
        );
        PluginsAndModules pluginsInfo = new PluginsAndModules(plugins, Collections.emptyList());

        final List<PluginInfo> infos = pluginsInfo.getPluginInfos();
        List<String> names = infos.stream().map(PluginInfo::getName).toList();
        assertThat(names, contains("a", "b", "c", "d", "e"));
    }

    public void testUnknownProperties() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "extra",
            "property",
            "unknown",
            "property",
            "description",
            "fake desc",
            "classname",
            "Foo",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version")
        );
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("Unknown properties for plugin [my_plugin] in plugin descriptor"));
    }

    public void testMissingType() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "classname",
            "Foo",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version")
        );

        final PluginInfo pluginInfo = PluginInfo.readFromProperties(pluginDir);
        assertThat(pluginInfo.getType(), equalTo(PluginType.ISOLATED));
    }

    public void testInvalidType() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "classname",
            "Foo",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "type",
            "invalid"
        );

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("[type] must be unspecified or one of [isolated, bootstrap] but found [invalid]"));
    }

    public void testJavaOptsAreAcceptedWithBootstrapPlugin() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "type",
            "bootstrap",
            "java.opts",
            "-Dfoo=bar"
        );

        final PluginInfo pluginInfo = PluginInfo.readFromProperties(pluginDir);
        assertThat(pluginInfo.getType(), equalTo(PluginType.BOOTSTRAP));
        assertThat(pluginInfo.getJavaOpts(), equalTo("-Dfoo=bar"));
    }

    public void testJavaOptsAreRejectedWithNonBootstrapPlugin() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "classname",
            "Foo",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "type",
            "isolated",
            "java.opts",
            "-Dfoo=bar"
        );

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("[java.opts] can only have a value when [type] is set to [bootstrap]"));
    }

    public void testClassnameIsRejectedWithBootstrapPlugin() throws Exception {
        Path pluginDir = createTempDir().resolve("fake-plugin");
        PluginTestUtil.writePluginProperties(
            pluginDir,
            "description",
            "fake desc",
            "classname",
            "Foo",
            "name",
            "my_plugin",
            "version",
            "1.0",
            "elasticsearch.version",
            Version.CURRENT.toString(),
            "java.version",
            System.getProperty("java.specification.version"),
            "type",
            "bootstrap"
        );

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> PluginInfo.readFromProperties(pluginDir));
        assertThat(e.getMessage(), containsString("[classname] can only have a value when [type] is set to [bootstrap]"));
    }

    /**
     * This is important because {@link PluginsUtils#getPluginBundles(Path)} will
     * use the hashcode to catch duplicate names
     */
    public void testSameNameSameHash() {
        PluginInfo info1 = new PluginInfo(
            "c",
            "foo",
            "dummy",
            Version.CURRENT,
            "1.8",
            "dummyclass",
            null,
            Collections.singletonList("foo"),
            randomBoolean(),
            PluginType.ISOLATED,
            "-Dfoo=bar",
            randomBoolean()
        );
        PluginInfo info2 = new PluginInfo(
            info1.getName(),
            randomValueOtherThan(info1.getDescription(), () -> randomAlphaOfLengthBetween(4, 12)),
            randomValueOtherThan(info1.getVersion(), () -> randomAlphaOfLengthBetween(4, 12)),
            info1.getElasticsearchVersion().previousMajor(),
            randomValueOtherThan(info1.getJavaVersion(), () -> randomAlphaOfLengthBetween(4, 12)),
            randomValueOtherThan(info1.getClassname(), () -> randomAlphaOfLengthBetween(4, 12)),
            randomAlphaOfLength(6),
            Collections.singletonList(
                randomValueOtherThanMany(v -> info1.getExtendedPlugins().contains(v), () -> randomAlphaOfLengthBetween(4, 12))
            ),
            info1.hasNativeController() == false,
            randomValueOtherThan(info1.getType(), () -> randomFrom(PluginType.values())),
            randomValueOtherThan(info1.getJavaOpts(), () -> randomAlphaOfLengthBetween(4, 12)),
            info1.isLicensed() == false
        );

        assertThat(info1.hashCode(), equalTo(info2.hashCode()));
    }

    public void testDifferentNameDifferentHash() {
        PluginInfo info1 = new PluginInfo(
            "c",
            "foo",
            "dummy",
            Version.CURRENT,
            "1.8",
            "dummyclass",
            null,
            Collections.singletonList("foo"),
            randomBoolean(),
            PluginType.ISOLATED,
            "-Dfoo=bar",
            randomBoolean()
        );
        PluginInfo info2 = new PluginInfo(
            randomValueOtherThan(info1.getName(), () -> randomAlphaOfLengthBetween(4, 12)),
            info1.getDescription(),
            info1.getVersion(),
            info1.getElasticsearchVersion(),
            info1.getJavaVersion(),
            info1.getClassname(),
            info1.getModuleName().orElse(null),
            info1.getExtendedPlugins(),
            info1.hasNativeController(),
            info1.getType(),
            info1.getJavaOpts(),
            info1.isLicensed()
        );

        assertThat(info1.hashCode(), not(equalTo(info2.hashCode())));
    }
}
