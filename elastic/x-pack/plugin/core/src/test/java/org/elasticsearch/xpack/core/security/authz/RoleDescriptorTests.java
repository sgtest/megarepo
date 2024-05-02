/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.security.authz;

import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.TransportVersion;
import org.elasticsearch.TransportVersions;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.cache.Cache;
import org.elasticsearch.common.io.stream.ByteBufferStreamInput;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.TestMatchers;
import org.elasticsearch.test.TransportVersionUtils;
import org.elasticsearch.xcontent.ToXContent;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentParserConfiguration;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.core.XPackClientPlugin;
import org.elasticsearch.xpack.core.security.authz.RoleDescriptor.ApplicationResourcePrivileges;
import org.elasticsearch.xpack.core.security.authz.permission.FieldPermissionsCache;
import org.elasticsearch.xpack.core.security.authz.permission.RemoteClusterPermissionGroup;
import org.elasticsearch.xpack.core.security.authz.permission.RemoteClusterPermissions;
import org.elasticsearch.xpack.core.security.authz.privilege.ClusterPrivilegeResolver;
import org.elasticsearch.xpack.core.security.authz.privilege.ConfigurableClusterPrivilege;
import org.elasticsearch.xpack.core.security.authz.privilege.ConfigurableClusterPrivileges;
import org.elasticsearch.xpack.core.security.authz.privilege.IndexPrivilege;
import org.elasticsearch.xpack.core.security.support.MetadataUtils;
import org.hamcrest.Matchers;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static org.elasticsearch.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.xpack.core.security.action.apikey.CrossClusterApiKeyRoleDescriptorBuilder.CCR_CLUSTER_PRIVILEGE_NAMES;
import static org.elasticsearch.xpack.core.security.action.apikey.CrossClusterApiKeyRoleDescriptorBuilder.CCR_INDICES_PRIVILEGE_NAMES;
import static org.elasticsearch.xpack.core.security.action.apikey.CrossClusterApiKeyRoleDescriptorBuilder.CCS_AND_CCR_CLUSTER_PRIVILEGE_NAMES;
import static org.elasticsearch.xpack.core.security.action.apikey.CrossClusterApiKeyRoleDescriptorBuilder.CCS_CLUSTER_PRIVILEGE_NAMES;
import static org.elasticsearch.xpack.core.security.action.apikey.CrossClusterApiKeyRoleDescriptorBuilder.CCS_INDICES_PRIVILEGE_NAMES;
import static org.elasticsearch.xpack.core.security.action.apikey.CrossClusterApiKeyRoleDescriptorBuilder.ROLE_DESCRIPTOR_NAME;
import static org.elasticsearch.xpack.core.security.authz.RoleDescriptor.WORKFLOWS_RESTRICTION_VERSION;
import static org.hamcrest.Matchers.arrayContaining;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.emptyArray;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.lessThan;
import static org.hamcrest.Matchers.nullValue;
import static org.hamcrest.core.Is.is;

public class RoleDescriptorTests extends ESTestCase {

    public void testIndexGroup() throws Exception {
        RoleDescriptor.IndicesPrivileges privs = RoleDescriptor.IndicesPrivileges.builder()
            .indices("idx")
            .privileges("priv")
            .allowRestrictedIndices(true)
            .build();
        XContentBuilder b = jsonBuilder();
        privs.toXContent(b, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"names\":[\"idx\"],\"privileges\":[\"priv\"],\"allow_restricted_indices\":true}", Strings.toString(b));
    }

    public void testRemoteIndexGroup() throws Exception {
        RoleDescriptor.RemoteIndicesPrivileges privs = RoleDescriptor.RemoteIndicesPrivileges.builder("remote")
            .indices("idx")
            .privileges("priv")
            .allowRestrictedIndices(true)
            .build();
        XContentBuilder b = jsonBuilder();
        privs.toXContent(b, ToXContent.EMPTY_PARAMS);
        assertEquals(
            "{\"names\":[\"idx\"],\"privileges\":[\"priv\"],\"allow_restricted_indices\":true,\"clusters\":[\"remote\"]}",
            Strings.toString(b)
        );
    }

    public void testRemoteIndexGroupThrowsOnEmptyClusters() {
        IllegalArgumentException ex = expectThrows(
            IllegalArgumentException.class,
            () -> RoleDescriptor.RemoteIndicesPrivileges.builder().indices("idx").privileges("priv").build()
        );
        assertThat(
            ex.getMessage(),
            containsString("the [remote_indices] sub-field [clusters] must refer to at least one cluster alias or cluster alias pattern")
        );
    }

    public void testEqualsOnEmptyRoles() {
        RoleDescriptor nullRoleDescriptor = new RoleDescriptor(
            "null_role",
            randomFrom((String[]) null, new String[0]),
            randomFrom((RoleDescriptor.IndicesPrivileges[]) null, new RoleDescriptor.IndicesPrivileges[0]),
            randomFrom((ApplicationResourcePrivileges[]) null, new ApplicationResourcePrivileges[0]),
            randomFrom((ConfigurableClusterPrivilege[]) null, new ConfigurableClusterPrivilege[0]),
            randomFrom((String[]) null, new String[0]),
            randomFrom((Map<String, Object>) null, Map.of()),
            Map.of("transient", "meta", "is", "ignored")
        );
        assertTrue(nullRoleDescriptor.equals(new RoleDescriptor("null_role", null, null, null, null, null, null, null)));
    }

    public void testToString() {
        RoleDescriptor.IndicesPrivileges[] groups = new RoleDescriptor.IndicesPrivileges[] {
            RoleDescriptor.IndicesPrivileges.builder()
                .indices("i1", "i2")
                .privileges("read")
                .grantedFields("body", "title")
                .query("{\"match_all\": {}}")
                .build() };
        final ApplicationResourcePrivileges[] applicationPrivileges = {
            ApplicationResourcePrivileges.builder().application("my_app").privileges("read", "write").resources("*").build() };

        final ConfigurableClusterPrivilege[] configurableClusterPrivileges = new ConfigurableClusterPrivilege[] {
            new ConfigurableClusterPrivileges.WriteProfileDataPrivileges(new LinkedHashSet<>(Arrays.asList("app*"))),
            new ConfigurableClusterPrivileges.ManageApplicationPrivileges(new LinkedHashSet<>(Arrays.asList("app01", "app02"))) };

        RoleDescriptor descriptor = new RoleDescriptor(
            "test",
            new String[] { "all", "none" },
            groups,
            applicationPrivileges,
            configurableClusterPrivileges,
            new String[] { "sudo" },
            Collections.emptyMap(),
            Collections.emptyMap()
        );

        assertThat(
            descriptor.toString(),
            is(
                "Role[name=test, cluster=[all,none]"
                    + ", global=[{APPLICATION:manage:applications=app01,app02},{PROFILE:write:applications=app*}]"
                    + ", indicesPrivileges=[IndicesPrivileges[indices=[i1,i2], allowRestrictedIndices=[false], privileges=[read]"
                    + ", field_security=[grant=[body,title], except=null], query={\"match_all\": {}}],]"
                    + ", applicationPrivileges=[ApplicationResourcePrivileges[application=my_app, privileges=[read,write], resources=[*]],]"
                    + ", runAs=[sudo], metadata=[{}], remoteIndicesPrivileges=[], remoteClusterPrivileges=[]"
                    + ", restriction=Restriction[workflows=[]]]"
            )
        );
    }

    public void testToXContentRoundtrip() throws Exception {
        final RoleDescriptor descriptor = randomRoleDescriptor(true, true, true, true);
        final XContentType xContentType = randomFrom(XContentType.values());
        final BytesReference xContentValue = toShuffledXContent(descriptor, xContentType, ToXContent.EMPTY_PARAMS, false);
        final RoleDescriptor parsed = RoleDescriptor.parserBuilder()
            .allowRestriction(true)
            .build()
            .parse(descriptor.getName(), xContentValue, xContentType);
        assertThat(parsed, equalTo(descriptor));
    }

    public void testParse() throws Exception {
        String q = "{\"cluster\":[\"a\", \"b\"]}";
        RoleDescriptor rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertArrayEquals(new String[] { "a", "b" }, rd.getClusterPrivileges());
        assertEquals(0, rd.getIndicesPrivileges().length);
        assertArrayEquals(Strings.EMPTY_ARRAY, rd.getRunAs());

        q = """
            {
              "cluster": [ "a", "b" ],
              "run_as": [ "m", "n" ]
            }""";
        rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertArrayEquals(new String[] { "a", "b" }, rd.getClusterPrivileges());
        assertEquals(0, rd.getIndicesPrivileges().length);
        assertArrayEquals(new String[] { "m", "n" }, rd.getRunAs());

        q = """
            {
              "cluster": [ "a", "b" ],
              "run_as": [ "m", "n" ],
              "index": [
                {
                  "names": "idx1",
                  "privileges": [ "p1", "p2" ]
                },
                {
                  "names": "idx2",
                  "allow_restricted_indices": true,
                  "privileges": [ "p3" ],
                  "field_security": {
                    "grant": [ "f1", "f2" ]
                  }
                },
                {
                  "names": "idx2",
                  "allow_restricted_indices": false,
                  "privileges": [ "p3" ],
                  "field_security": {
                    "grant": [ "f1", "f2" ]
                  },
                  "query": {
                    "match_all": {}
                  }
                }
              ],
              "remote_indices": [
                {
                  "names": "idx1",
                  "privileges": [ "p1", "p2" ],
                  "clusters": ["r1"]
                },
                {
                  "names": "idx2",
                  "allow_restricted_indices": true,
                  "privileges": [ "p3" ],
                  "field_security": {
                    "grant": [ "f1", "f2" ]
                  },
                  "clusters": ["r1", "*-*"]
                },
                {
                  "names": "idx2",
                  "allow_restricted_indices": false,
                  "privileges": [ "p3" ],
                  "field_security": {
                    "grant": [ "f1", "f2" ]
                  },
                  "query": {
                    "match_all": {}
                  },
                  "clusters": ["*"]
                }
              ],
              "remote_cluster": [
                {
                  "privileges": [
                      "monitor_enrich"
                  ],
                  "clusters": [
                      "one"
                  ]
                },
                {
                  "privileges": [
                      "monitor_enrich"
                  ],
                  "clusters": [
                      "two", "three"
                  ]
                }
              ],
              "restriction":{
                "workflows": ["search_application_query"]
              }
            }""";
        rd = RoleDescriptor.parserBuilder().allowRestriction(true).build().parse("test", new BytesArray(q), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertArrayEquals(new String[] { "a", "b" }, rd.getClusterPrivileges());
        assertEquals(3, rd.getIndicesPrivileges().length);
        assertEquals(3, rd.getRemoteIndicesPrivileges().length);
        assertEquals(2, rd.getRemoteClusterPermissions().groups().size());
        assertArrayEquals(new String[] { "r1" }, rd.getRemoteIndicesPrivileges()[0].remoteClusters());
        assertArrayEquals(new String[] { "r1", "*-*" }, rd.getRemoteIndicesPrivileges()[1].remoteClusters());
        assertArrayEquals(new String[] { "*" }, rd.getRemoteIndicesPrivileges()[2].remoteClusters());
        assertArrayEquals(new String[] { "m", "n" }, rd.getRunAs());
        assertArrayEquals(new String[] { "one" }, rd.getRemoteClusterPermissions().groups().get(0).remoteClusterAliases());
        assertArrayEquals(new String[] { "monitor_enrich" }, rd.getRemoteClusterPermissions().groups().get(0).clusterPrivileges());
        assertArrayEquals(new String[] { "two", "three" }, rd.getRemoteClusterPermissions().groups().get(1).remoteClusterAliases());
        assertArrayEquals(new String[] { "monitor_enrich" }, rd.getRemoteClusterPermissions().groups().get(1).clusterPrivileges());
        assertThat(rd.hasRestriction(), equalTo(true));
        assertThat(rd.getRestriction().hasWorkflows(), equalTo(true));
        assertArrayEquals(new String[] { "search_application_query" }, rd.getRestriction().getWorkflows());
        q = """
            {
              "cluster": [ "a", "b" ],
              "run_as": [ "m", "n" ],
              "index": [
                {
                  "names": [ "idx1", "idx2" ],
                  "privileges": [ "p1", "p2" ],
                  "allow_restricted_indices": true
                }
              ],
              "global": {
                "profile": {
                }
              }
            }""";
        rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertArrayEquals(new String[] { "a", "b" }, rd.getClusterPrivileges());
        assertEquals(1, rd.getIndicesPrivileges().length);
        assertArrayEquals(new String[] { "idx1", "idx2" }, rd.getIndicesPrivileges()[0].getIndices());
        assertTrue(rd.getIndicesPrivileges()[0].allowRestrictedIndices());
        assertArrayEquals(new String[] { "m", "n" }, rd.getRunAs());
        assertNull(rd.getIndicesPrivileges()[0].getQuery());

        q = """
            {"cluster":["a", "b"], "metadata":{"foo":"bar"}}""";
        rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertArrayEquals(new String[] { "a", "b" }, rd.getClusterPrivileges());
        assertEquals(0, rd.getIndicesPrivileges().length);
        assertArrayEquals(Strings.EMPTY_ARRAY, rd.getRunAs());
        assertNotNull(rd.getMetadata());
        assertThat(rd.getMetadata().size(), is(1));
        assertThat(rd.getMetadata().get("foo"), is("bar"));

        q = """
            {
              "cluster": [ "a", "b" ],
              "run_as": [ "m", "n" ],
              "index": [
                {
                  "names": [ "idx1", "idx2" ],
                  "allow_restricted_indices": false,
                  "privileges": [ "p1", "p2" ]
                }
              ],
              "applications": [
                {
                  "resources": [ "object-123", "object-456" ],
                  "privileges": [ "read", "delete" ],
                  "application": "app1"
                },
                {
                  "resources": [ "*" ],
                  "privileges": [ "admin" ],
                  "application": "app2"
                }
              ],
              "global": {
                "application": {
                  "manage": {
                    "applications": [ "kibana", "logstash" ]
                  }
                },
                "profile": {
                }
              }
            }""";
        rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q), XContentType.JSON);
        assertThat(rd.getName(), equalTo("test"));
        assertThat(rd.getClusterPrivileges(), arrayContaining("a", "b"));
        assertThat(rd.getIndicesPrivileges().length, equalTo(1));
        assertThat(rd.getIndicesPrivileges()[0].getIndices(), arrayContaining("idx1", "idx2"));
        assertThat(rd.getIndicesPrivileges()[0].allowRestrictedIndices(), is(false));
        assertThat(rd.getIndicesPrivileges()[0].getQuery(), nullValue());
        assertThat(rd.getRunAs(), arrayContaining("m", "n"));
        assertThat(rd.getApplicationPrivileges().length, equalTo(2));
        assertThat(rd.getApplicationPrivileges()[0].getResources(), arrayContaining("object-123", "object-456"));
        assertThat(rd.getApplicationPrivileges()[0].getPrivileges(), arrayContaining("read", "delete"));
        assertThat(rd.getApplicationPrivileges()[0].getApplication(), equalTo("app1"));
        assertThat(rd.getApplicationPrivileges()[1].getResources(), arrayContaining("*"));
        assertThat(rd.getApplicationPrivileges()[1].getPrivileges(), arrayContaining("admin"));
        assertThat(rd.getApplicationPrivileges()[1].getApplication(), equalTo("app2"));
        assertThat(rd.getConditionalClusterPrivileges(), Matchers.arrayWithSize(1));

        ConfigurableClusterPrivilege conditionalPrivilege = rd.getConditionalClusterPrivileges()[0];
        assertThat(conditionalPrivilege.getCategory(), equalTo(ConfigurableClusterPrivilege.Category.APPLICATION));
        assertThat(conditionalPrivilege, instanceOf(ConfigurableClusterPrivileges.ManageApplicationPrivileges.class));
        assertThat(
            ((ConfigurableClusterPrivileges.ManageApplicationPrivileges) conditionalPrivilege).getApplicationNames(),
            containsInAnyOrder("kibana", "logstash")
        );

        q = """
            {
              "cluster": [ "manage" ],
              "global": {
                "profile": {
                  "write": {
                    "applications": [ "", "kibana-*" ]
                  }
                },
                "application": {
                  "manage": {
                    "applications": [ "apm*", "kibana-1" ]
                  }
                }
              }
            }""";
        rd = RoleDescriptor.parserBuilder().build().parse("testUpdateProfile", new BytesArray(q), XContentType.JSON);
        assertThat(rd.getName(), is("testUpdateProfile"));
        assertThat(rd.getClusterPrivileges(), arrayContaining("manage"));
        assertThat(rd.getIndicesPrivileges(), Matchers.emptyArray());
        assertThat(rd.getRunAs(), Matchers.emptyArray());
        assertThat(rd.getApplicationPrivileges(), Matchers.emptyArray());
        assertThat(rd.getConditionalClusterPrivileges(), Matchers.arrayWithSize(2));

        conditionalPrivilege = rd.getConditionalClusterPrivileges()[0];
        assertThat(conditionalPrivilege.getCategory(), equalTo(ConfigurableClusterPrivilege.Category.APPLICATION));
        assertThat(conditionalPrivilege, instanceOf(ConfigurableClusterPrivileges.ManageApplicationPrivileges.class));
        assertThat(
            ((ConfigurableClusterPrivileges.ManageApplicationPrivileges) conditionalPrivilege).getApplicationNames(),
            containsInAnyOrder("apm*", "kibana-1")
        );
        conditionalPrivilege = rd.getConditionalClusterPrivileges()[1];
        assertThat(conditionalPrivilege.getCategory(), equalTo(ConfigurableClusterPrivilege.Category.PROFILE));
        assertThat(conditionalPrivilege, instanceOf(ConfigurableClusterPrivileges.WriteProfileDataPrivileges.class));
        assertThat(
            ((ConfigurableClusterPrivileges.WriteProfileDataPrivileges) conditionalPrivilege).getApplicationNames(),
            containsInAnyOrder("", "kibana-*")
        );

        q = """
            {"applications": [{"application": "myapp", "resources": ["*"], "privileges": ["login" ]}] }""";
        rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q), XContentType.JSON);
        assertThat(rd.getName(), equalTo("test"));
        assertThat(rd.getClusterPrivileges(), emptyArray());
        assertThat(rd.getIndicesPrivileges(), emptyArray());
        assertThat(rd.getApplicationPrivileges().length, equalTo(1));
        assertThat(rd.getApplicationPrivileges()[0].getResources(), arrayContaining("*"));
        assertThat(rd.getApplicationPrivileges()[0].getPrivileges(), arrayContaining("login"));
        assertThat(rd.getApplicationPrivileges()[0].getApplication(), equalTo("myapp"));
        assertThat(rd.getConditionalClusterPrivileges(), Matchers.arrayWithSize(0));

        final String badJson = """
            {"applications":[{"not_supported": true, "resources": ["*"], "privileges": ["my-app:login" ]}] }""";
        final IllegalArgumentException ex = expectThrows(
            IllegalArgumentException.class,
            () -> RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(badJson), XContentType.JSON)
        );
        assertThat(ex.getMessage(), containsString("not_supported"));

        rd = RoleDescriptor.parserBuilder().allowRestriction(true).build().parse("test_empty_restriction", new BytesArray("""
            {
              "index": [{"names": "idx1", "privileges": [ "p1", "p2" ]}],
              "restriction":{}
            }"""), XContentType.JSON);
        assertThat(rd.getName(), equalTo("test_empty_restriction"));
        assertThat(rd.hasRestriction(), equalTo(false));
        assertThat(rd.hasWorkflowsRestriction(), equalTo(false));

        final ElasticsearchParseException pex1 = expectThrows(
            ElasticsearchParseException.class,
            () -> RoleDescriptor.parserBuilder().allowRestriction(true).build().parse("test_null_workflows", new BytesArray("""
                {
                  "index": [{"names": ["idx1"], "privileges": [ "p1", "p2" ]}],
                  "restriction":{"workflows":null}
                }"""), XContentType.JSON)
        );
        assertThat(
            pex1.getMessage(),
            containsString(
                "failed to parse restriction for role [test_null_workflows]. could not parse [workflows] field. "
                    + "expected a string array but found null value instead"
            )
        );

        final ElasticsearchParseException pex2 = expectThrows(
            ElasticsearchParseException.class,
            () -> RoleDescriptor.parserBuilder().allowRestriction(true).build().parse("test_empty_workflows", new BytesArray("""
                {
                  "index": [{"names": ["idx1"], "privileges": [ "p1", "p2" ]}],
                  "restriction":{"workflows":[]}
                }"""), XContentType.JSON)
        );
        assertThat(
            pex2.getMessage(),
            containsString("failed to parse restriction for role [test_empty_workflows]. [workflows] cannot be an empty array")
        );
    }

    public void testParseInvalidRemoteCluster() throws IOException {
        // missing clusters
        String q = """
            {
              "remote_cluster": [
                {
                  "privileges": [
                      "monitor_enrich"
                  ]
                }
              ]
            }""";
        ElasticsearchParseException exception = expectThrows(
            ElasticsearchParseException.class,
            () -> RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q), XContentType.JSON)
        );
        assertThat(
            exception.getMessage(),
            containsString("failed to parse remote_cluster for role [test]. " + "[clusters] must be defined when [privileges] are defined")
        );

        // missing privileges
        String q2 = """
            {
              "remote_cluster": [
                {
                  "clusters": [
                      "two", "three"
                  ]
                }
              ]
            }""";
        exception = expectThrows(
            ElasticsearchParseException.class,
            () -> RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q2), XContentType.JSON)
        );
        assertThat(
            exception.getMessage(),
            containsString("failed to parse remote_cluster for role [test]. " + "[privileges] must be defined when [clusters] are defined")
        );

        // missing both does not cause an exception while parsing. However, we generally want to avoid any assumptions about the behavior
        // and is allowed for legacy reasons to better match how other fields work
        String q3 = """
            {
              "remote_cluster": []
            }""";
        RoleDescriptor rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q3), XContentType.JSON);
        assertThat(rd.getRemoteClusterPermissions().groups().size(), equalTo(0));
        assertThat(rd.getRemoteClusterPermissions(), equalTo(RemoteClusterPermissions.NONE));
        if (assertsAreEnabled) {
            expectThrows(AssertionError.class, () -> rd.getRemoteClusterPermissions().validate());
        }
        // similarly, missing both but with a group placeholder does not cause an exception while parsing but will still raise an exception
        String q4 = """
            {
              "remote_cluster": [{}]
            }""";

        IllegalArgumentException illegalArgumentException = expectThrows(
            IllegalArgumentException.class,
            () -> RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(q4), XContentType.JSON)
        );
        assertThat(illegalArgumentException.getMessage(), containsString("remote cluster groups must not be null or empty"));
    }

    public void testParsingFieldPermissionsUsesCache() throws IOException {
        FieldPermissionsCache fieldPermissionsCache = new FieldPermissionsCache(Settings.EMPTY);
        RoleDescriptor.setFieldPermissionsCache(fieldPermissionsCache);

        final Cache.CacheStats beforeStats = fieldPermissionsCache.getCacheStats();

        final String json = """
            {
              "index": [
                {
                  "names": "index-001",
                  "privileges": [ "read" ],
                  "field_security": {
                    "grant": [ "field-001", "field-002" ]
                  }
                },
                {
                  "names": "index-001",
                  "privileges": [ "read" ],
                  "field_security": {
                    "grant": [ "*" ],
                    "except": [ "field-003" ]
                  }
                }
              ]
            }
            """;
        RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON);

        final int numberOfFieldSecurityBlocks = 2;
        final Cache.CacheStats betweenStats = fieldPermissionsCache.getCacheStats();
        assertThat(betweenStats.getMisses(), equalTo(beforeStats.getMisses() + numberOfFieldSecurityBlocks));
        assertThat(betweenStats.getHits(), equalTo(beforeStats.getHits()));

        final int iterations = randomIntBetween(1, 5);
        for (int i = 0; i < iterations; i++) {
            RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON);
        }

        final Cache.CacheStats afterStats = fieldPermissionsCache.getCacheStats();
        assertThat(afterStats.getMisses(), equalTo(betweenStats.getMisses()));
        assertThat(afterStats.getHits(), equalTo(beforeStats.getHits() + numberOfFieldSecurityBlocks * iterations));
    }

    public void testSerializationForCurrentVersion() throws Exception {
        final TransportVersion version = TransportVersionUtils.randomCompatibleVersion(random());
        final boolean canIncludeRemoteIndices = version.onOrAfter(TransportVersions.V_8_8_0);
        final boolean canIncludeRemoteClusters = version.onOrAfter(TransportVersions.ROLE_REMOTE_CLUSTER_PRIVS);
        final boolean canIncludeWorkflows = version.onOrAfter(WORKFLOWS_RESTRICTION_VERSION);
        logger.info("Testing serialization with version {}", version);
        BytesStreamOutput output = new BytesStreamOutput();
        output.setTransportVersion(version);

        final RoleDescriptor descriptor = randomRoleDescriptor(
            true,
            canIncludeRemoteIndices,
            canIncludeWorkflows,
            canIncludeRemoteClusters
        );
        descriptor.writeTo(output);
        final NamedWriteableRegistry registry = new NamedWriteableRegistry(new XPackClientPlugin().getNamedWriteables());
        StreamInput streamInput = new NamedWriteableAwareStreamInput(
            ByteBufferStreamInput.wrap(BytesReference.toBytes(output.bytes())),
            registry
        );
        streamInput.setTransportVersion(version);
        final RoleDescriptor serialized = new RoleDescriptor(streamInput);

        assertThat(serialized, equalTo(descriptor));
    }

    public void testSerializationWithRemoteIndicesWithElderVersion() throws IOException {
        final TransportVersion versionBeforeRemoteIndices = TransportVersionUtils.getPreviousVersion(TransportVersions.V_8_8_0);
        final TransportVersion version = TransportVersionUtils.randomVersionBetween(
            random(),
            TransportVersions.V_7_17_0,
            versionBeforeRemoteIndices
        );
        final BytesStreamOutput output = new BytesStreamOutput();
        output.setTransportVersion(version);

        final RoleDescriptor descriptor = randomRoleDescriptor(true, true, false, false);
        descriptor.writeTo(output);
        final NamedWriteableRegistry registry = new NamedWriteableRegistry(new XPackClientPlugin().getNamedWriteables());
        StreamInput streamInput = new NamedWriteableAwareStreamInput(
            ByteBufferStreamInput.wrap(BytesReference.toBytes(output.bytes())),
            registry
        );
        streamInput.setTransportVersion(version);
        final RoleDescriptor serialized = new RoleDescriptor(streamInput);
        if (descriptor.hasRemoteIndicesPrivileges()) {
            assertThat(
                serialized,
                equalTo(
                    new RoleDescriptor(
                        descriptor.getName(),
                        descriptor.getClusterPrivileges(),
                        descriptor.getIndicesPrivileges(),
                        descriptor.getApplicationPrivileges(),
                        descriptor.getConditionalClusterPrivileges(),
                        descriptor.getRunAs(),
                        descriptor.getMetadata(),
                        descriptor.getTransientMetadata(),
                        null,
                        null,
                        descriptor.getRestriction()
                    )
                )
            );
        } else {
            assertThat(descriptor, equalTo(serialized));
        }
    }

    public void testSerializationWithRemoteClusterWithElderVersion() throws IOException {
        final TransportVersion versionBeforeRemoteCluster = TransportVersionUtils.getPreviousVersion(
            TransportVersions.ROLE_REMOTE_CLUSTER_PRIVS
        );
        final TransportVersion version = TransportVersionUtils.randomVersionBetween(
            random(),
            TransportVersions.V_7_17_0,
            versionBeforeRemoteCluster
        );
        final BytesStreamOutput output = new BytesStreamOutput();
        output.setTransportVersion(version);

        final RoleDescriptor descriptor = randomRoleDescriptor(true, false, false, true);
        descriptor.writeTo(output);
        final NamedWriteableRegistry registry = new NamedWriteableRegistry(new XPackClientPlugin().getNamedWriteables());
        StreamInput streamInput = new NamedWriteableAwareStreamInput(
            ByteBufferStreamInput.wrap(BytesReference.toBytes(output.bytes())),
            registry
        );
        streamInput.setTransportVersion(version);
        final RoleDescriptor serialized = new RoleDescriptor(streamInput);
        if (descriptor.hasRemoteClusterPermissions()) {
            assertThat(
                serialized,
                equalTo(
                    new RoleDescriptor(
                        descriptor.getName(),
                        descriptor.getClusterPrivileges(),
                        descriptor.getIndicesPrivileges(),
                        descriptor.getApplicationPrivileges(),
                        descriptor.getConditionalClusterPrivileges(),
                        descriptor.getRunAs(),
                        descriptor.getMetadata(),
                        descriptor.getTransientMetadata(),
                        null,
                        descriptor.getRemoteClusterPermissions(),
                        descriptor.getRestriction()
                    )
                )
            );
        } else {
            assertThat(descriptor, equalTo(serialized));
            assertThat(descriptor.getRemoteClusterPermissions(), equalTo(RemoteClusterPermissions.NONE));
        }
    }

    public void testSerializationWithWorkflowsRestrictionAndUnsupportedVersions() throws IOException {
        final TransportVersion versionBeforeWorkflowsRestriction = TransportVersionUtils.getPreviousVersion(WORKFLOWS_RESTRICTION_VERSION);
        final TransportVersion version = TransportVersionUtils.randomVersionBetween(
            random(),
            TransportVersions.V_7_17_0,
            versionBeforeWorkflowsRestriction
        );
        final BytesStreamOutput output = new BytesStreamOutput();
        output.setTransportVersion(version);

        final RoleDescriptor descriptor = randomRoleDescriptor(true, false, true, false);
        descriptor.writeTo(output);
        final NamedWriteableRegistry registry = new NamedWriteableRegistry(new XPackClientPlugin().getNamedWriteables());
        StreamInput streamInput = new NamedWriteableAwareStreamInput(
            ByteBufferStreamInput.wrap(BytesReference.toBytes(output.bytes())),
            registry
        );
        streamInput.setTransportVersion(version);
        final RoleDescriptor serialized = new RoleDescriptor(streamInput);
        if (descriptor.hasWorkflowsRestriction()) {
            assertThat(
                serialized,
                equalTo(
                    new RoleDescriptor(
                        descriptor.getName(),
                        descriptor.getClusterPrivileges(),
                        descriptor.getIndicesPrivileges(),
                        descriptor.getApplicationPrivileges(),
                        descriptor.getConditionalClusterPrivileges(),
                        descriptor.getRunAs(),
                        descriptor.getMetadata(),
                        descriptor.getTransientMetadata(),
                        descriptor.getRemoteIndicesPrivileges(),
                        descriptor.getRemoteClusterPermissions(),
                        null
                    )
                )
            );
        } else {
            assertThat(descriptor, equalTo(serialized));
        }
    }

    public void testParseRoleWithRestrictionFailsWhenAllowRestrictionIsFalse() {
        final String json = """
            {
              "restriction": {
                 "workflows": ["search_application"]
              }
            }""";
        final ElasticsearchParseException e = expectThrows(
            ElasticsearchParseException.class,
            () -> RoleDescriptor.parserBuilder()
                .allowRestriction(false)
                .build()
                .parse(
                    "test_role_with_restriction",
                    XContentHelper.createParser(XContentParserConfiguration.EMPTY, new BytesArray(json), XContentType.JSON)
                )
        );
        assertThat(
            e,
            TestMatchers.throwableWithMessage(
                containsString("failed to parse role [test_role_with_restriction]. unexpected field [restriction]")
            )
        );
    }

    public void testParseRoleWithRestrictionWhenAllowRestrictionIsTrue() throws IOException {
        final String json = """
            {
              "restriction": {
                 "workflows": ["search_application"]
              }
            }""";
        RoleDescriptor role = RoleDescriptor.parserBuilder()
            .allowRestriction(true)
            .build()
            .parse(
                "test_role_with_restriction",
                XContentHelper.createParser(XContentParserConfiguration.EMPTY, new BytesArray(json), XContentType.JSON)
            );
        assertThat(role.getName(), equalTo("test_role_with_restriction"));
        assertThat(role.hasRestriction(), equalTo(true));
        assertThat(role.hasWorkflowsRestriction(), equalTo(true));
        assertThat(role.getRestriction().getWorkflows(), arrayContaining("search_application"));
    }

    public void testParseEmptyQuery() throws Exception {
        String json = """
            {
                "cluster": [ "a", "b" ],
                "run_as": [ "m", "n" ],
                "index": [
                  {
                    "names": [ "idx1", "idx2" ],
                    "privileges": [ "p1", "p2" ],
                    "query": ""
                  }
                ]
              }""";
        RoleDescriptor rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertArrayEquals(new String[] { "a", "b" }, rd.getClusterPrivileges());
        assertEquals(1, rd.getIndicesPrivileges().length);
        assertArrayEquals(new String[] { "idx1", "idx2" }, rd.getIndicesPrivileges()[0].getIndices());
        assertArrayEquals(new String[] { "m", "n" }, rd.getRunAs());
        assertNull(rd.getIndicesPrivileges()[0].getQuery());
    }

    public void testParseNullQuery() throws Exception {
        String json = """
            {
              "cluster": [ "a", "b" ],
              "run_as": [ "m", "n" ],
              "index": [
                {
                  "names": [ "idx1", "idx2" ],
                  "privileges": [ "p1", "p2" ],
                  "query": null
                }
              ]
            }""";
        RoleDescriptor rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertArrayEquals(new String[] { "a", "b" }, rd.getClusterPrivileges());
        assertEquals(1, rd.getIndicesPrivileges().length);
        assertArrayEquals(new String[] { "idx1", "idx2" }, rd.getIndicesPrivileges()[0].getIndices());
        assertArrayEquals(new String[] { "m", "n" }, rd.getRunAs());
        assertNull(rd.getIndicesPrivileges()[0].getQuery());
    }

    public void testParseEmptyQueryUsingDeprecatedIndicesField() throws Exception {
        String json = """
            {
              "cluster": [ "a", "b" ],
              "run_as": [ "m", "n" ],
              "indices": [
                {
                  "names": [ "idx1", "idx2" ],
                  "privileges": [ "p1", "p2" ],
                  "query": ""
                }
              ]
            }""";
        RoleDescriptor rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertArrayEquals(new String[] { "a", "b" }, rd.getClusterPrivileges());
        assertEquals(1, rd.getIndicesPrivileges().length);
        assertArrayEquals(new String[] { "idx1", "idx2" }, rd.getIndicesPrivileges()[0].getIndices());
        assertArrayEquals(new String[] { "m", "n" }, rd.getRunAs());
        assertNull(rd.getIndicesPrivileges()[0].getQuery());
    }

    public void testParseIgnoresTransientMetadata() throws Exception {
        final RoleDescriptor descriptor = new RoleDescriptor(
            "test",
            new String[] { "all" },
            null,
            null,
            null,
            null,
            Collections.singletonMap("_unlicensed_feature", true),
            Collections.singletonMap("foo", "bar")
        );
        XContentBuilder b = jsonBuilder();
        descriptor.toXContent(b, ToXContent.EMPTY_PARAMS);
        RoleDescriptor parsed = RoleDescriptor.parserBuilder().build().parse("test", BytesReference.bytes(b), XContentType.JSON);
        assertNotNull(parsed);
        assertEquals(1, parsed.getTransientMetadata().size());
        assertEquals(true, parsed.getTransientMetadata().get("enabled"));
    }

    public void testParseIndicesPrivilegesSucceedsWhenExceptFieldsIsSubsetOfGrantedFields() throws IOException {
        final boolean grantAll = randomBoolean();
        final String grant = grantAll ? "\"*\"" : "\"f1\",\"f2\"";
        final String except = grantAll ? "\"_fx\",\"f8\"" : "\"f1\"";

        final String json = Strings.format("""
            {
              "indices": [
                {
                  "names": [ "idx1", "idx2" ],
                  "privileges": [ "p1", "p2" ],
                  "field_security": {
                    "grant": [ %s ],
                    "except": [ %s ]
                  }
                }
              ]
            }""", grant, except);
        final RoleDescriptor rd = RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON);
        assertEquals("test", rd.getName());
        assertEquals(1, rd.getIndicesPrivileges().length);
        assertArrayEquals(new String[] { "idx1", "idx2" }, rd.getIndicesPrivileges()[0].getIndices());
        assertArrayEquals((grantAll) ? new String[] { "*" } : new String[] { "f1", "f2" }, rd.getIndicesPrivileges()[0].getGrantedFields());
        assertArrayEquals(
            (grantAll) ? new String[] { "_fx", "f8" } : new String[] { "f1" },
            rd.getIndicesPrivileges()[0].getDeniedFields()
        );
    }

    public void testParseIndicesPrivilegesFailsWhenExceptFieldsAreNotSubsetOfGrantedFields() {
        resetFieldPermssionsCache();

        final String json = """
            {
              "indices": [
                {
                  "names": [ "idx1", "idx2" ],
                  "privileges": [ "p1", "p2" ],
                  "field_security": {
                    "grant": [ "f1", "f2" ],
                    "except": [ "f3" ]
                  }
                }
              ]
            }""";
        final ElasticsearchParseException epe = expectThrows(
            ElasticsearchParseException.class,
            () -> RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON)
        );
        assertThat(epe, TestMatchers.throwableWithMessage(containsString("must be a subset of the granted fields ")));
        assertThat(epe, TestMatchers.throwableWithMessage(containsString("f1")));
        assertThat(epe, TestMatchers.throwableWithMessage(containsString("f2")));
        assertThat(epe, TestMatchers.throwableWithMessage(containsString("f3")));
    }

    public void testParseRemoteIndicesPrivilegesFailsWhenClustersFieldMissing() {
        final String json = """
            {
              "remote_indices": [
                {
                  "names": [ "idx1", "idx2" ],
                  "privileges": [ "all" ]
                }
              ]
            }""";
        final ElasticsearchParseException epe = expectThrows(
            ElasticsearchParseException.class,
            () -> RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON)
        );
        assertThat(
            epe,
            TestMatchers.throwableWithMessage(
                containsString("failed to parse remote indices privileges for role [test]. missing required [clusters] field")
            )
        );
    }

    public void testParseIndicesPrivilegesFailsWhenClustersFieldPresent() {
        final String json = """
            {
              "indices": [
                {
                  "names": [ "idx1", "idx2" ],
                  "privileges": [ "all" ],
                  "clusters": ["remote"]
                }
              ]
            }""";
        final ElasticsearchParseException epe = expectThrows(
            ElasticsearchParseException.class,
            () -> RoleDescriptor.parserBuilder().build().parse("test", new BytesArray(json), XContentType.JSON)
        );
        assertThat(
            epe,
            TestMatchers.throwableWithMessage(
                containsString("failed to parse indices privileges for role [test]. unexpected field [clusters]")
            )
        );
    }

    public void testIndicesPrivilegesCompareTo() {
        final RoleDescriptor.IndicesPrivileges indexPrivilege = randomIndicesPrivilegesBuilder().build();
        @SuppressWarnings({ "EqualsWithItself" })
        final int actual = indexPrivilege.compareTo(indexPrivilege);
        assertThat(actual, equalTo(0));
        assertThat(
            indexPrivilege.compareTo(
                RoleDescriptor.IndicesPrivileges.builder()
                    .indices(indexPrivilege.getIndices().clone())
                    .privileges(indexPrivilege.getPrivileges().clone())
                    // test for both cases when the query is the same instance or a copy
                    .query(
                        (indexPrivilege.getQuery() == null || randomBoolean())
                            ? indexPrivilege.getQuery()
                            : new BytesArray(indexPrivilege.getQuery().toBytesRef())
                    )
                    .grantedFields(indexPrivilege.getGrantedFields() == null ? null : indexPrivilege.getGrantedFields().clone())
                    .deniedFields(indexPrivilege.getDeniedFields() == null ? null : indexPrivilege.getDeniedFields().clone())
                    .allowRestrictedIndices(indexPrivilege.allowRestrictedIndices())
                    .build()
            ),
            equalTo(0)
        );

        RoleDescriptor.IndicesPrivileges first = randomIndicesPrivilegesBuilder().allowRestrictedIndices(false).build();
        RoleDescriptor.IndicesPrivileges second = randomIndicesPrivilegesBuilder().allowRestrictedIndices(true).build();
        assertThat(first.compareTo(second), lessThan(0));
        assertThat(second.compareTo(first), greaterThan(0));

        first = randomIndicesPrivilegesBuilder().indices("a", "b").build();
        second = randomIndicesPrivilegesBuilder().indices("b", "a").allowRestrictedIndices(first.allowRestrictedIndices()).build();
        assertThat(first.compareTo(second), lessThan(0));
        assertThat(second.compareTo(first), greaterThan(0));

        first = randomIndicesPrivilegesBuilder().privileges("read", "write").build();
        second = randomIndicesPrivilegesBuilder().allowRestrictedIndices(first.allowRestrictedIndices())
            .privileges("write", "read")
            .indices(first.getIndices())
            .build();
        assertThat(first.compareTo(second), lessThan(0));
        assertThat(second.compareTo(first), greaterThan(0));

        first = randomIndicesPrivilegesBuilder().query(randomBoolean() ? null : "{\"match\":{\"field-a\":\"a\"}}").build();
        second = randomIndicesPrivilegesBuilder().allowRestrictedIndices(first.allowRestrictedIndices())
            .query("{\"match\":{\"field-b\":\"b\"}}")
            .indices(first.getIndices())
            .privileges(first.getPrivileges())
            .build();
        assertThat(first.compareTo(second), lessThan(0));
        assertThat(second.compareTo(first), greaterThan(0));

        first = randomIndicesPrivilegesBuilder().grantedFields(randomBoolean() ? null : new String[] { "a", "b" }).build();
        second = randomIndicesPrivilegesBuilder().allowRestrictedIndices(first.allowRestrictedIndices())
            .grantedFields("b", "a")
            .indices(first.getIndices())
            .privileges(first.getPrivileges())
            .query(first.getQuery())
            .build();
        assertThat(first.compareTo(second), lessThan(0));
        assertThat(second.compareTo(first), greaterThan(0));

        first = randomIndicesPrivilegesBuilder().deniedFields(randomBoolean() ? null : new String[] { "a", "b" }).build();
        second = randomIndicesPrivilegesBuilder().allowRestrictedIndices(first.allowRestrictedIndices())
            .deniedFields("b", "a")
            .indices(first.getIndices())
            .privileges(first.getPrivileges())
            .query(first.getQuery())
            .grantedFields(first.getGrantedFields())
            .build();
        assertThat(first.compareTo(second), lessThan(0));
        assertThat(second.compareTo(first), greaterThan(0));
    }

    public void testGlobalPrivilegesOrdering() throws IOException {
        final String roleName = randomAlphaOfLengthBetween(3, 30);
        final String[] applicationNames = generateRandomStringArray(3, randomIntBetween(0, 3), false, true);
        final String[] profileNames = generateRandomStringArray(3, randomIntBetween(0, 3), false, true);
        ConfigurableClusterPrivilege[] configurableClusterPrivileges = new ConfigurableClusterPrivilege[] {
            new ConfigurableClusterPrivileges.WriteProfileDataPrivileges(Sets.newHashSet(profileNames)),
            new ConfigurableClusterPrivileges.ManageApplicationPrivileges(Sets.newHashSet(applicationNames)) };
        RoleDescriptor role1 = new RoleDescriptor(
            roleName,
            new String[0],
            new RoleDescriptor.IndicesPrivileges[0],
            new RoleDescriptor.ApplicationResourcePrivileges[0],
            configurableClusterPrivileges,
            new String[0],
            Map.of(),
            Map.of()
        );
        // swap
        var temp = configurableClusterPrivileges[0];
        configurableClusterPrivileges[0] = configurableClusterPrivileges[1];
        configurableClusterPrivileges[1] = temp;
        RoleDescriptor role2 = new RoleDescriptor(
            roleName,
            new String[0],
            new RoleDescriptor.IndicesPrivileges[0],
            new RoleDescriptor.ApplicationResourcePrivileges[0],
            configurableClusterPrivileges,
            new String[0],
            Map.of(),
            Map.of()
        );
        assertThat(role2, is(role1));
        StringBuilder applicationNamesString = new StringBuilder();
        for (int i = 0; i < applicationNames.length; i++) {
            if (i > 0) {
                applicationNamesString.append(", ");
            }
            applicationNamesString.append("\"" + applicationNames[i] + "\"");
        }
        StringBuilder profileNamesString = new StringBuilder();
        for (int i = 0; i < profileNames.length; i++) {
            if (i > 0) {
                profileNamesString.append(", ");
            }
            profileNamesString.append("\"" + profileNames[i] + "\"");
        }
        String json = Strings.format("""
            {
              "global": {
                "profile": {
                  "write": {
                    "applications": [ %s ]
                  }
                },
                "application": {
                  "manage": {
                    "applications": [ %s ]
                  }
                }
              }
            }""", profileNamesString, applicationNamesString);
        RoleDescriptor role3 = RoleDescriptor.parserBuilder().build().parse(roleName, new BytesArray(json), XContentType.JSON);
        assertThat(role3, is(role1));
        json = Strings.format("""
            {
              "global": {
                "application": {
                  "manage": {
                    "applications": [ %s ]
                  }
                },
                "profile": {
                  "write": {
                    "applications": [ %s ]
                  }
                }
              }
            }""", applicationNamesString, profileNamesString);
        RoleDescriptor role4 = RoleDescriptor.parserBuilder().build().parse(roleName, new BytesArray(json), XContentType.JSON);
        assertThat(role4, is(role1));
    }

    public void testIsEmpty() {
        assertTrue(new RoleDescriptor(randomAlphaOfLengthBetween(1, 10), null, null, null, null, null, null, null).isEmpty());

        assertTrue(
            new RoleDescriptor(
                randomAlphaOfLengthBetween(1, 10),
                new String[0],
                new RoleDescriptor.IndicesPrivileges[0],
                new ApplicationResourcePrivileges[0],
                new ConfigurableClusterPrivilege[0],
                new String[0],
                new HashMap<>(),
                new HashMap<>(),
                new RoleDescriptor.RemoteIndicesPrivileges[0],
                RemoteClusterPermissions.NONE,
                null
            ).isEmpty()
        );

        final List<Boolean> booleans = Arrays.asList(
            randomBoolean(),
            randomBoolean(),
            randomBoolean(),
            randomBoolean(),
            randomBoolean(),
            randomBoolean(),
            randomBoolean(),
            randomBoolean(),
            randomBoolean()
        );

        final RoleDescriptor roleDescriptor = new RoleDescriptor(
            randomAlphaOfLengthBetween(1, 10),
            booleans.get(0) ? new String[0] : new String[] { "foo" },
            booleans.get(1)
                ? new RoleDescriptor.IndicesPrivileges[0]
                : new RoleDescriptor.IndicesPrivileges[] {
                    RoleDescriptor.IndicesPrivileges.builder().indices("idx").privileges("foo").build() },
            booleans.get(2)
                ? new ApplicationResourcePrivileges[0]
                : new ApplicationResourcePrivileges[] {
                    ApplicationResourcePrivileges.builder().application("app").privileges("foo").resources("res").build() },
            booleans.get(3)
                ? new ConfigurableClusterPrivilege[0]
                : new ConfigurableClusterPrivilege[] {
                    randomBoolean()
                        ? new ConfigurableClusterPrivileges.ManageApplicationPrivileges(Collections.singleton("foo"))
                        : new ConfigurableClusterPrivileges.WriteProfileDataPrivileges(Collections.singleton("bar")) },
            booleans.get(4) ? new String[0] : new String[] { "foo" },
            booleans.get(5) ? new HashMap<>() : Collections.singletonMap("foo", "bar"),
            Collections.singletonMap("foo", "bar"),
            booleans.get(6)
                ? new RoleDescriptor.RemoteIndicesPrivileges[0]
                : new RoleDescriptor.RemoteIndicesPrivileges[] {
                    RoleDescriptor.RemoteIndicesPrivileges.builder("rmt").indices("idx").privileges("foo").build() },
            booleans.get(7) ? null : randomRemoteClusterPermissions(5),
            booleans.get(8) ? null : RoleRestrictionTests.randomWorkflowsRestriction(1, 2)
        );

        if (booleans.stream().anyMatch(e -> e.equals(false))) {
            assertFalse(roleDescriptor.isEmpty());
        } else {
            assertTrue(roleDescriptor.isEmpty());
        }
    }

    public void testHasPrivilegesOtherThanIndex() {
        assertThat(
            new RoleDescriptor(
                "name",
                null,
                randomBoolean() ? null : randomIndicesPrivileges(1, 5),
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                null
            ).hasUnsupportedPrivilegesInsideAPIKeyConnectedRemoteCluster(),
            is(false)
        );
        final RoleDescriptor roleDescriptor = randomRoleDescriptor();
        final boolean expected = roleDescriptor.hasClusterPrivileges()
            || roleDescriptor.hasConfigurableClusterPrivileges()
            || roleDescriptor.hasApplicationPrivileges()
            || roleDescriptor.hasRunAs()
            || roleDescriptor.hasRemoteIndicesPrivileges();
        assertThat(roleDescriptor.hasUnsupportedPrivilegesInsideAPIKeyConnectedRemoteCluster(), equalTo(expected));
    }

    public static List<RoleDescriptor> randomUniquelyNamedRoleDescriptors(int minSize, int maxSize) {
        return randomValueOtherThanMany(
            roleDescriptors -> roleDescriptors.stream().map(RoleDescriptor::getName).distinct().count() != roleDescriptors.size(),
            () -> randomList(minSize, maxSize, () -> randomRoleDescriptor(false))
        );
    }

    public static RoleDescriptor randomRoleDescriptor() {
        return randomRoleDescriptor(true);
    }

    public static RoleDescriptor randomRoleDescriptor(boolean allowReservedMetadata) {
        return randomRoleDescriptor(allowReservedMetadata, false, false, false);
    }

    public static RoleDescriptor randomRoleDescriptor(
        boolean allowReservedMetadata,
        boolean allowRemoteIndices,
        boolean allowWorkflows,
        boolean allowRemoteClusters
    ) {
        final RoleDescriptor.RemoteIndicesPrivileges[] remoteIndexPrivileges;
        if (false == allowRemoteIndices || randomBoolean()) {
            remoteIndexPrivileges = null;
        } else {
            remoteIndexPrivileges = randomRemoteIndicesPrivileges(0, 3);
        }

        RemoteClusterPermissions remoteClusters = RemoteClusterPermissions.NONE;
        if (allowRemoteClusters && randomBoolean()) {
            randomRemoteClusterPermissions(randomIntBetween(1, 5));
        }

        return new RoleDescriptor(
            randomAlphaOfLengthBetween(3, 90),
            randomSubsetOf(ClusterPrivilegeResolver.names()).toArray(String[]::new),
            randomIndicesPrivileges(0, 3),
            randomApplicationPrivileges(),
            randomClusterPrivileges(),
            generateRandomStringArray(5, randomIntBetween(2, 8), false, true),
            randomRoleDescriptorMetadata(allowReservedMetadata),
            Map.of(),
            remoteIndexPrivileges,
            remoteClusters,
            allowWorkflows ? RoleRestrictionTests.randomWorkflowsRestriction(1, 3) : null
        );
    }

    public static Map<String, Object> randomRoleDescriptorMetadata(boolean allowReservedMetadata) {
        final Map<String, Object> metadata = new HashMap<>();
        while (randomBoolean()) {
            String key = randomAlphaOfLengthBetween(4, 12);
            if (allowReservedMetadata && randomBoolean()) {
                key = MetadataUtils.RESERVED_PREFIX + key;
            }
            final Object value = randomBoolean() ? randomInt() : randomAlphaOfLengthBetween(3, 50);
            metadata.put(key, value);
        }
        return metadata;
    }

    public static ConfigurableClusterPrivilege[] randomClusterPrivileges() {
        final ConfigurableClusterPrivilege[] configurableClusterPrivileges = switch (randomIntBetween(0, 4)) {
            case 0 -> new ConfigurableClusterPrivilege[0];
            case 1 -> new ConfigurableClusterPrivilege[] {
                new ConfigurableClusterPrivileges.ManageApplicationPrivileges(
                    Sets.newHashSet(generateRandomStringArray(3, randomIntBetween(4, 12), false, false))
                ) };
            case 2 -> new ConfigurableClusterPrivilege[] {
                new ConfigurableClusterPrivileges.WriteProfileDataPrivileges(
                    Sets.newHashSet(generateRandomStringArray(3, randomIntBetween(4, 12), false, false))
                ) };
            case 3 -> new ConfigurableClusterPrivilege[] {
                new ConfigurableClusterPrivileges.WriteProfileDataPrivileges(
                    Sets.newHashSet(generateRandomStringArray(3, randomIntBetween(4, 12), false, false))
                ),
                new ConfigurableClusterPrivileges.ManageApplicationPrivileges(
                    Sets.newHashSet(generateRandomStringArray(3, randomIntBetween(4, 12), false, false))
                ) };
            case 4 -> new ConfigurableClusterPrivilege[] {
                new ConfigurableClusterPrivileges.ManageApplicationPrivileges(
                    Sets.newHashSet(generateRandomStringArray(3, randomIntBetween(4, 12), false, false))
                ),
                new ConfigurableClusterPrivileges.WriteProfileDataPrivileges(
                    Sets.newHashSet(generateRandomStringArray(3, randomIntBetween(4, 12), false, false))
                ) };
            default -> throw new IllegalStateException("Unexpected value");
        };
        return configurableClusterPrivileges;
    }

    public static ApplicationResourcePrivileges[] randomApplicationPrivileges() {
        final ApplicationResourcePrivileges[] applicationPrivileges = new ApplicationResourcePrivileges[randomIntBetween(0, 2)];
        for (int i = 0; i < applicationPrivileges.length; i++) {
            final ApplicationResourcePrivileges.Builder builder = ApplicationResourcePrivileges.builder();
            builder.application("app" + randomAlphaOfLengthBetween(5, 12) + (randomBoolean() ? "*" : ""));
            if (randomBoolean()) {
                builder.privileges("*");
            } else {
                builder.privileges(generateRandomStringArray(6, randomIntBetween(4, 8), false, false));
            }
            if (randomBoolean()) {
                builder.resources("*");
            } else {
                builder.resources(generateRandomStringArray(6, randomIntBetween(4, 8), false, false));
            }
            applicationPrivileges[i] = builder.build();
        }
        return applicationPrivileges;
    }

    public static RemoteClusterPermissions randomRemoteClusterPermissions(int maxGroups) {
        final RemoteClusterPermissions remoteClusterPermissions = new RemoteClusterPermissions();
        final String[] supportedPermissions = RemoteClusterPermissions.getSupportedRemoteClusterPermissions().toArray(new String[0]);
        for (int i = 0; i < maxGroups; i++) {
            remoteClusterPermissions.addGroup(
                new RemoteClusterPermissionGroup(
                    randomNonEmptySubsetOf(Arrays.asList(supportedPermissions)).toArray(new String[0]),
                    generateRandomStringArray(5, randomIntBetween(3, 9), false, false)
                )
            );
        }
        return remoteClusterPermissions;
    }

    public static RoleDescriptor.RemoteIndicesPrivileges[] randomRemoteIndicesPrivileges(int min, int max) {
        return randomRemoteIndicesPrivileges(min, max, Set.of());
    }

    public static RoleDescriptor.RemoteIndicesPrivileges[] randomRemoteIndicesPrivileges(int min, int max, Set<String> excludedPrivileges) {
        final RoleDescriptor.IndicesPrivileges[] innerIndexPrivileges = randomIndicesPrivileges(min, max, excludedPrivileges);
        final RoleDescriptor.RemoteIndicesPrivileges[] remoteIndexPrivileges =
            new RoleDescriptor.RemoteIndicesPrivileges[innerIndexPrivileges.length];
        for (int i = 0; i < remoteIndexPrivileges.length; i++) {
            remoteIndexPrivileges[i] = new RoleDescriptor.RemoteIndicesPrivileges(
                innerIndexPrivileges[i],
                generateRandomStringArray(5, randomIntBetween(3, 9), false, false)
            );
        }
        return remoteIndexPrivileges;
    }

    public static RoleDescriptor.IndicesPrivileges[] randomIndicesPrivileges(int min, int max) {
        return randomIndicesPrivileges(min, max, Set.of());
    }

    public static RoleDescriptor.IndicesPrivileges[] randomIndicesPrivileges(int min, int max, Set<String> excludedPrivileges) {
        final RoleDescriptor.IndicesPrivileges[] indexPrivileges = new RoleDescriptor.IndicesPrivileges[randomIntBetween(min, max)];
        for (int i = 0; i < indexPrivileges.length; i++) {
            indexPrivileges[i] = randomIndicesPrivilegesBuilder(excludedPrivileges).build();
        }
        return indexPrivileges;
    }

    private static RoleDescriptor.IndicesPrivileges.Builder randomIndicesPrivilegesBuilder() {
        return randomIndicesPrivilegesBuilder(Set.of());
    }

    private static RoleDescriptor.IndicesPrivileges.Builder randomIndicesPrivilegesBuilder(Set<String> excludedPrivileges) {
        final Set<String> candidatePrivilegesNames = Sets.difference(IndexPrivilege.names(), excludedPrivileges);
        assert false == candidatePrivilegesNames.isEmpty() : "no candidate privilege names to random from";
        final RoleDescriptor.IndicesPrivileges.Builder builder = RoleDescriptor.IndicesPrivileges.builder()
            .privileges(randomSubsetOf(randomIntBetween(1, 4), candidatePrivilegesNames))
            .indices(generateRandomStringArray(5, randomIntBetween(3, 9), false, false))
            .allowRestrictedIndices(randomBoolean());
        randomDlsFls(builder);
        return builder;
    }

    private static void randomDlsFls(RoleDescriptor.IndicesPrivileges.Builder builder) {
        if (randomBoolean()) {
            builder.query(
                randomBoolean()
                    ? "{ \"term\": { \"" + randomAlphaOfLengthBetween(3, 24) + "\" : \"" + randomAlphaOfLengthBetween(3, 24) + "\" }"
                    : "{ \"match_all\": {} }"
            );
        }
        if (randomBoolean()) {
            if (randomBoolean()) {
                builder.grantedFields("*");
                builder.deniedFields(generateRandomStringArray(4, randomIntBetween(4, 9), false, false));
            } else {
                builder.grantedFields(generateRandomStringArray(4, randomIntBetween(4, 9), false, false));
            }
        }
    }

    private static void resetFieldPermssionsCache() {
        RoleDescriptor.setFieldPermissionsCache(new FieldPermissionsCache(Settings.EMPTY));
    }

    public static RoleDescriptor randomCrossClusterAccessRoleDescriptor() {
        final int searchSize = randomIntBetween(0, 3);
        final int replicationSize = randomIntBetween(searchSize == 0 ? 1 : 0, 3);
        assert searchSize + replicationSize > 0;

        final String[] clusterPrivileges;
        if (searchSize > 0 && replicationSize > 0) {
            clusterPrivileges = CCS_AND_CCR_CLUSTER_PRIVILEGE_NAMES;
        } else if (searchSize > 0) {
            clusterPrivileges = CCS_CLUSTER_PRIVILEGE_NAMES;
        } else {
            clusterPrivileges = CCR_CLUSTER_PRIVILEGE_NAMES;
        }

        final List<RoleDescriptor.IndicesPrivileges> indexPrivileges = new ArrayList<>();
        for (int i = 0; i < searchSize; i++) {
            final RoleDescriptor.IndicesPrivileges.Builder builder = RoleDescriptor.IndicesPrivileges.builder()
                .privileges(CCS_INDICES_PRIVILEGE_NAMES)
                .indices(generateRandomStringArray(5, randomIntBetween(3, 9), false, false))
                .allowRestrictedIndices(randomBoolean());
            randomDlsFls(builder);
            indexPrivileges.add(builder.build());
        }
        for (int i = 0; i < replicationSize; i++) {
            final RoleDescriptor.IndicesPrivileges.Builder builder = RoleDescriptor.IndicesPrivileges.builder()
                .privileges(CCR_INDICES_PRIVILEGE_NAMES)
                .indices(generateRandomStringArray(5, randomIntBetween(3, 9), false, false))
                .allowRestrictedIndices(randomBoolean());
            indexPrivileges.add(builder.build());
        }

        return new RoleDescriptor(
            ROLE_DESCRIPTOR_NAME,
            clusterPrivileges,
            indexPrivileges.toArray(RoleDescriptor.IndicesPrivileges[]::new),
            null
        );
    }

}
