/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.node;

import org.elasticsearch.Version;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.xcontent.ToXContentFragment;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.node.Node;

import java.io.IOException;
import java.util.Collection;
import java.util.Collections;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.SortedSet;
import java.util.TreeSet;
import java.util.function.Function;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static org.elasticsearch.node.NodeRoleSettings.NODE_ROLES_SETTING;


/**
 * A discovery node represents a node that is part of the cluster.
 */
public class DiscoveryNode implements Writeable, ToXContentFragment {

    static final String COORDINATING_ONLY = "coordinating_only";

    public static boolean hasRole(final Settings settings, final DiscoveryNodeRole role) {
        /*
         * This method can be called before the o.e.n.NodeRoleSettings.NODE_ROLES_SETTING is initialized. We do not want to trigger
         * initialization prematurely because that will bake the default roles before plugins have had a chance to register them. Therefore,
         * to avoid initializing this setting prematurely, we avoid using the actual node roles setting instance here.
         */
        if (settings.hasValue("node.roles")) {
            return settings.getAsList("node.roles").contains(role.roleName());
        } else if (role.legacySetting() != null && settings.hasValue(role.legacySetting().getKey())) {
            return role.legacySetting().get(settings);
        } else {
            return role.isEnabledByDefault(settings);
        }
    }

    public static boolean isMasterNode(final Settings settings) {
        return hasRole(settings, DiscoveryNodeRole.MASTER_ROLE);
    }

    /**
     * Due to the way that plugins may not be available when settings are being initialized,
     * not all roles may be available from a static/initializing context such as a {@link Setting}
     * default value function. In that case, be warned that this may not include all plugin roles.
     */
    public static boolean isDataNode(final Settings settings) {
        return getRolesFromSettings(settings).stream().anyMatch(DiscoveryNodeRole::canContainData);
    }

    /**
     * Allows determining the "data" property without the need to load plugins, but does this purely based on
     * naming conventions. Prefer using {@link #isDataNode(Settings)} if possible.
     */
    public static boolean isDataNodeBasedOnNamingConvention(final Settings settings) {
        return DiscoveryNode.hasRole(settings, DiscoveryNodeRole.DATA_ROLE) ||
            settings.getAsList("node.roles").stream().anyMatch(DiscoveryNodeRole::isDataRoleBasedOnNamingConvention);
    }

    public static boolean isIngestNode(final Settings settings) {
        return hasRole(settings, DiscoveryNodeRole.INGEST_ROLE);
    }

    public static boolean isRemoteClusterClient(final Settings settings) {
        return hasRole(settings, DiscoveryNodeRole.REMOTE_CLUSTER_CLIENT_ROLE);
    }

    private final String nodeName;
    private final String nodeId;
    private final String ephemeralId;
    private final String hostName;
    private final String hostAddress;
    private final TransportAddress address;
    private final Map<String, String> attributes;
    private final Version version;
    private final SortedSet<DiscoveryNodeRole> roles;

    /**
     * Creates a new {@link DiscoveryNode}
     * <p>
     * <b>Note:</b> if the version of the node is unknown {@link Version#minimumCompatibilityVersion()} should be used for the current
     * version. it corresponds to the minimum version this elasticsearch version can communicate with. If a higher version is used
     * the node might not be able to communicate with the remote node. After initial handshakes node versions will be discovered
     * and updated.
     * </p>
     *
     * @param id               the nodes unique (persistent) node id. This constructor will auto generate a random ephemeral id.
     * @param address          the nodes transport address
     * @param version          the version of the node
     */
    public DiscoveryNode(final String id, TransportAddress address, Version version) {
        this(id, address, Collections.emptyMap(), DiscoveryNodeRole.BUILT_IN_ROLES, version);
    }

    /**
     * Creates a new {@link DiscoveryNode}
     * <p>
     * <b>Note:</b> if the version of the node is unknown {@link Version#minimumCompatibilityVersion()} should be used for the current
     * version. it corresponds to the minimum version this elasticsearch version can communicate with. If a higher version is used
     * the node might not be able to communicate with the remote node. After initial handshakes node versions will be discovered
     * and updated.
     * </p>
     *
     * @param id               the nodes unique (persistent) node id. This constructor will auto generate a random ephemeral id.
     * @param address          the nodes transport address
     * @param attributes       node attributes
     * @param roles            node roles
     * @param version          the version of the node
     */
    public DiscoveryNode(String id, TransportAddress address, Map<String, String> attributes, Set<DiscoveryNodeRole> roles,
                         Version version) {
        this("", id, address, attributes, roles, version);
    }

    /**
     * Creates a new {@link DiscoveryNode}
     * <p>
     * <b>Note:</b> if the version of the node is unknown {@link Version#minimumCompatibilityVersion()} should be used for the current
     * version. it corresponds to the minimum version this elasticsearch version can communicate with. If a higher version is used
     * the node might not be able to communicate with the remote node. After initial handshakes node versions will be discovered
     * and updated.
     * </p>
     *
     * @param nodeName         the nodes name
     * @param nodeId           the nodes unique persistent id. An ephemeral id will be auto generated.
     * @param address          the nodes transport address
     * @param attributes       node attributes
     * @param roles            node roles
     * @param version          the version of the node
     */
    public DiscoveryNode(String nodeName, String nodeId, TransportAddress address,
                         Map<String, String> attributes, Set<DiscoveryNodeRole> roles, Version version) {
        this(nodeName, nodeId, UUIDs.randomBase64UUID(), address.address().getHostString(), address.getAddress(), address, attributes,
            roles, version);
    }

    /**
     * Creates a new {@link DiscoveryNode}.
     * <p>
     * <b>Note:</b> if the version of the node is unknown {@link Version#minimumCompatibilityVersion()} should be used for the current
     * version. it corresponds to the minimum version this elasticsearch version can communicate with. If a higher version is used
     * the node might not be able to communicate with the remote node. After initial handshakes node versions will be discovered
     * and updated.
     * </p>
     *
     * @param nodeName         the nodes name
     * @param nodeId           the nodes unique persistent id
     * @param ephemeralId      the nodes unique ephemeral id
     * @param hostAddress      the nodes host address
     * @param address          the nodes transport address
     * @param attributes       node attributes
     * @param roles            node roles
     * @param version          the version of the node
     */
    public DiscoveryNode(String nodeName, String nodeId, String ephemeralId, String hostName, String hostAddress,
                         TransportAddress address, Map<String, String> attributes, Set<DiscoveryNodeRole> roles, Version version) {
        if (nodeName != null) {
            this.nodeName = nodeName.intern();
        } else {
            this.nodeName = "";
        }
        this.nodeId = nodeId.intern();
        this.ephemeralId = ephemeralId.intern();
        this.hostName = hostName.intern();
        this.hostAddress = hostAddress.intern();
        this.address = address;
        if (version == null) {
            this.version = Version.CURRENT;
        } else {
            this.version = version;
        }
        this.attributes = Collections.unmodifiableMap(attributes);
        assert DiscoveryNode.roleMap.values().stream().noneMatch(role -> attributes.containsKey(role.roleName())) :
                "Node roles must not be provided as attributes but saw attributes " + attributes;
        this.roles = Collections.unmodifiableSortedSet(new TreeSet<>(roles));
    }

    /** Creates a DiscoveryNode representing the local node. */
    public static DiscoveryNode createLocal(Settings settings, TransportAddress publishAddress, String nodeId) {
        Map<String, String> attributes = Node.NODE_ATTRIBUTES.getAsMap(settings);
        Set<DiscoveryNodeRole> roles = getRolesFromSettings(settings);
        return new DiscoveryNode(Node.NODE_NAME_SETTING.get(settings), nodeId, publishAddress, attributes, roles, Version.CURRENT);
    }

    /** extract node roles from the given settings */
    public static Set<DiscoveryNodeRole> getRolesFromSettings(final Settings settings) {
        if (NODE_ROLES_SETTING.exists(settings)) {
            validateLegacySettings(settings, roleMap);
            return Set.copyOf(NODE_ROLES_SETTING.get(settings));
        } else {
            return roleMap.values()
                .stream()
                .filter(s -> s.legacySetting() != null && s.legacySetting().get(settings))
                .collect(Collectors.toUnmodifiableSet());
        }
    }

    private static void validateLegacySettings(final Settings settings, final Map<String, DiscoveryNodeRole> roleMap) {
        for (final DiscoveryNodeRole role : roleMap.values()) {
            if (role.legacySetting() != null && role.legacySetting().exists(settings)) {
                final String message = String.format(
                    Locale.ROOT,
                    "can not explicitly configure node roles and use legacy role setting [%s]=[%s]",
                    role.legacySetting().getKey(),
                    role.legacySetting().get(settings)
                );
                throw new IllegalArgumentException(message);
            }
        }
    }

    /**
     * Creates a new {@link DiscoveryNode} by reading from the stream provided as argument
     * @param in the stream
     * @throws IOException if there is an error while reading from the stream
     */
    public DiscoveryNode(StreamInput in) throws IOException {
        this.nodeName = in.readString().intern();
        this.nodeId = in.readString().intern();
        this.ephemeralId = in.readString().intern();
        this.hostName = in.readString().intern();
        this.hostAddress = in.readString().intern();
        this.address = new TransportAddress(in);
        this.attributes = in.readMap(StreamInput::readString, StreamInput::readString);
        int rolesSize = in.readVInt();
        final SortedSet<DiscoveryNodeRole> roles = new TreeSet<>();
        for (int i = 0; i < rolesSize; i++) {
            final String roleName = in.readString();
            final String roleNameAbbreviation = in.readString();
            final boolean canContainData;
            if (in.getVersion().onOrAfter(Version.V_7_10_0)) {
                canContainData = in.readBoolean();
            } else {
                canContainData = roleName.equals(DiscoveryNodeRole.DATA_ROLE.roleName());
            }
            final DiscoveryNodeRole role = roleMap.get(roleName);
            if (role == null) {
                roles.add(new DiscoveryNodeRole.UnknownRole(roleName, roleNameAbbreviation, canContainData));
            } else {
                assert roleName.equals(role.roleName()) : "role name [" + roleName + "] does not match role [" + role.roleName() + "]";
                assert roleNameAbbreviation.equals(role.roleNameAbbreviation())
                        : "role name abbreviation [" + roleName + "] does not match role [" + role.roleNameAbbreviation() + "]";
                roles.add(role);
            }
        }
        this.roles = Collections.unmodifiableSortedSet(roles);
        this.version = Version.readVersion(in);
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeString(nodeName);
        out.writeString(nodeId);
        out.writeString(ephemeralId);
        out.writeString(hostName);
        out.writeString(hostAddress);
        address.writeTo(out);
        out.writeMap(attributes, StreamOutput::writeString, StreamOutput::writeString);
        out.writeVInt(roles.size());
        for (final DiscoveryNodeRole role : roles) {
            out.writeString(role.roleName());
            out.writeString(role.roleNameAbbreviation());
            if (out.getVersion().onOrAfter(Version.V_7_10_0)) {
                out.writeBoolean(role.canContainData());
            }
        }
        Version.writeVersion(version, out);
    }

    /**
     * The address that the node can be communicated with.
     */
    public TransportAddress getAddress() {
        return address;
    }

    /**
     * The unique id of the node.
     */
    public String getId() {
        return nodeId;
    }

    /**
     * The unique ephemeral id of the node. Ephemeral ids are meant to be attached the life span
     * of a node process. When ever a node is restarted, it's ephemeral id is required to change (while it's {@link #getId()}
     * will be read from the data folder and will remain the same across restarts). Since all node attributes and addresses
     * are maintained during the life span of a node process, we can (and are) using the ephemeralId in
     * {@link DiscoveryNode#equals(Object)}.
     */
    public String getEphemeralId() {
        return ephemeralId;
    }

    /**
     * The name of the node.
     */
    public String getName() {
        return this.nodeName;
    }

    /**
     * The node attributes.
     */
    public Map<String, String> getAttributes() {
        return this.attributes;
    }

    /**
     * Should this node hold data (shards) or not.
     */
    public boolean isDataNode() {
        return roles.stream().anyMatch(DiscoveryNodeRole::canContainData);
    }

    /**
     * Can this node become master or not.
     */
    public boolean isMasterNode() {
        return roles.contains(DiscoveryNodeRole.MASTER_ROLE);
    }

    /**
     * Returns a boolean that tells whether this an ingest node or not
     */
    public boolean isIngestNode() {
        return roles.contains(DiscoveryNodeRole.INGEST_ROLE);
    }

    /**
     * Returns whether or not the node can be a remote cluster client.
     *
     * @return true if the node can be a remote cluster client, false otherwise
     */
    public boolean isRemoteClusterClient() {
        return roles.contains(DiscoveryNodeRole.REMOTE_CLUSTER_CLIENT_ROLE);
    }

    /**
     * Returns a set of all the roles that the node has. The roles are returned in sorted order by the role name.
     * <p>
     * If a node does not have any specific role, the returned set is empty, which means that the node is a coordinating-only node.
     *
     * @return the sorted set of roles
     */
    public Set<DiscoveryNodeRole> getRoles() {
        return roles;
    }

    public Version getVersion() {
        return this.version;
    }

    public String getHostName() {
        return this.hostName;
    }

    public String getHostAddress() {
        return this.hostAddress;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }

        DiscoveryNode that = (DiscoveryNode) o;

        return ephemeralId.equals(that.ephemeralId);
    }

    @Override
    public int hashCode() {
        // we only need to hash the id because it's highly unlikely that two nodes
        // in our system will have the same id but be different
        // This is done so that this class can be used efficiently as a key in a map
        return ephemeralId.hashCode();
    }

    @Override
    public String toString() {
        StringBuilder sb = new StringBuilder();
        if (nodeName.length() > 0) {
            sb.append('{').append(nodeName).append('}');
        }
        sb.append('{').append(nodeId).append('}');
        sb.append('{').append(ephemeralId).append('}');
        sb.append('{').append(hostName).append('}');
        sb.append('{').append(address).append('}');
        if (roles.isEmpty() == false) {
            sb.append('{');
            roles.stream().map(DiscoveryNodeRole::roleNameAbbreviation).sorted().forEach(sb::append);
            sb.append('}');
        }
        if (attributes.isEmpty() == false) {
            sb.append(attributes);
        }
        return sb.toString();
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject(getId());
        builder.field("name", getName());
        builder.field("ephemeral_id", getEphemeralId());
        builder.field("transport_address", getAddress().toString());

        builder.startObject("attributes");
        for (Map.Entry<String, String> entry : attributes.entrySet()) {
            builder.field(entry.getKey(), entry.getValue());
        }
        builder.endObject();

        builder.endObject();
        return builder;
    }

    private static Map<String, DiscoveryNodeRole> rolesToMap(final Stream<DiscoveryNodeRole> roles) {
        return roles.collect(Collectors.toUnmodifiableMap(DiscoveryNodeRole::roleName, Function.identity()));
    }

    private static Map<String, DiscoveryNodeRole> roleMap = rolesToMap(DiscoveryNodeRole.BUILT_IN_ROLES.stream());

    public static DiscoveryNodeRole getRoleFromRoleName(final String roleName) {
        if (roleMap.containsKey(roleName) == false) {
            throw new IllegalArgumentException("unknown role [" + roleName + "]");
        }
        return roleMap.get(roleName);
    }

    public static Collection<DiscoveryNodeRole> getPossibleRoles() {
        return roleMap.values();
    }

    public static void setAdditionalRoles(final Set<DiscoveryNodeRole> additionalRoles) {
        assert additionalRoles.stream().allMatch(r -> r.legacySetting() == null || r.legacySetting().isDeprecated()) : additionalRoles;
        final Map<String, DiscoveryNodeRole> roleNameToPossibleRoles =
            rolesToMap(Stream.concat(DiscoveryNodeRole.BUILT_IN_ROLES.stream(), additionalRoles.stream()));
        // collect the abbreviation names into a map to ensure that there are not any duplicate abbreviations
        final Map<String, DiscoveryNodeRole> roleNameAbbreviationToPossibleRoles = roleNameToPossibleRoles.values()
                .stream()
                .collect(Collectors.toUnmodifiableMap(DiscoveryNodeRole::roleNameAbbreviation, Function.identity()));
        assert roleNameToPossibleRoles.size() == roleNameAbbreviationToPossibleRoles.size() :
                "roles by name [" + roleNameToPossibleRoles + "], roles by name abbreviation [" + roleNameAbbreviationToPossibleRoles + "]";
        roleMap = roleNameToPossibleRoles;
    }

    public static Set<String> getPossibleRoleNames() {
        return roleMap.keySet();
    }
}
