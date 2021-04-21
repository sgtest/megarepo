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
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.xcontent.ToXContentFragment;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.node.Node;

import java.io.IOException;
import java.util.Collections;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.SortedSet;
import java.util.TreeSet;

import static org.elasticsearch.node.NodeRoleSettings.NODE_ROLES_SETTING;


/**
 * A discovery node represents a node that is part of the cluster.
 */
public class DiscoveryNode implements Writeable, ToXContentFragment {

    static final String COORDINATING_ONLY = "coordinating_only";

    public static boolean hasRole(final Settings settings, final DiscoveryNodeRole role) {
        // this method can be called before the o.e.n.NodeRoleSettings.NODE_ROLES_SETTING is initialized
        if (settings.hasValue("node.roles")) {
            return settings.getAsList("node.roles").contains(role.roleName());
        } else {
            return role.isEnabledByDefault(settings);
        }
    }

    public static boolean isMasterNode(final Settings settings) {
        return hasRole(settings, DiscoveryNodeRole.MASTER_ROLE);
    }

    /**
     * Check if the given settings are indicative of having the top-level data role.
     *
     * Note that if you want to test for whether or not the given settings are indicative of any role that can contain data, you should use
     * {@link #canContainData(Settings)}.
     *
     * @param settings the settings
     * @return true if the given settings are indicative of having the top-level data role, otherwise false
     */
    public static boolean hasDataRole(final Settings settings) {
        return hasRole(settings, DiscoveryNodeRole.DATA_ROLE);
    }

    /**
     * Check if the given settings are indicative of any role that can contain data.
     *
     * Note that if you want to test for exactly the data role, you should use {@link #hasDataRole(Settings)}.
     *
     * @param settings the settings
     * @return true if the given settings are indicative of having any role that can contain data, otherwise false
     */
    public static boolean canContainData(final Settings settings) {
        return getRolesFromSettings(settings).stream().anyMatch(DiscoveryNodeRole::canContainData);
    }

    public static boolean isIngestNode(final Settings settings) {
        return hasRole(settings, DiscoveryNodeRole.INGEST_ROLE);
    }

    public static boolean isRemoteClusterClient(final Settings settings) {
        return hasRole(settings, DiscoveryNodeRole.REMOTE_CLUSTER_CLIENT_ROLE);
    }

    private static boolean isDedicatedFrozenRoles(Set<DiscoveryNodeRole> roles) {
        return roles.contains(DiscoveryNodeRole.DATA_FROZEN_NODE_ROLE) &&
            roles.stream().filter(DiscoveryNodeRole::canContainData)
                .anyMatch(r -> r != DiscoveryNodeRole.DATA_FROZEN_NODE_ROLE) == false;
    }

    /**
     * Check if the settings are for a dedicated frozen node, i.e. has frozen role and no other data roles.
     */
    public static boolean isDedicatedFrozenNode(final Settings settings) {
        return isDedicatedFrozenRoles(getRolesFromSettings(settings));
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
        this(id, address, Collections.emptyMap(), DiscoveryNodeRole.roles(), version);
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
        assert DiscoveryNodeRole.roleNames().stream().noneMatch(attributes::containsKey) :
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
        return Set.copyOf(NODE_ROLES_SETTING.get(settings));
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
            final boolean canContainData = in.readBoolean();
            final Optional<DiscoveryNodeRole> maybeRole = DiscoveryNodeRole.maybeGetRoleFromRoleName(roleName);
            if (maybeRole.isEmpty()) {
                roles.add(new DiscoveryNodeRole.UnknownRole(roleName, roleNameAbbreviation, canContainData));
            } else {
                final DiscoveryNodeRole role = maybeRole.get();
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
            out.writeBoolean(role.canContainData());
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
    public boolean canContainData() {
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
     * Returns whether or not the node is a frozen only node, i.e., has data frozen role and no other data roles.
     * @return
     */
    public boolean isDedicatedFrozenNode() {
        return isDedicatedFrozenRoles(getRoles());
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
        appendDescriptionWithoutAttributes(sb);
        if (attributes.isEmpty() == false) {
            sb.append(attributes);
        }
        return sb.toString();
    }

    public void appendDescriptionWithoutAttributes(StringBuilder stringBuilder) {
        if (nodeName.length() > 0) {
            stringBuilder.append('{').append(nodeName).append('}');
        }
        stringBuilder.append('{').append(nodeId).append('}');
        stringBuilder.append('{').append(ephemeralId).append('}');
        stringBuilder.append('{').append(hostName).append('}');
        stringBuilder.append('{').append(address).append('}');
        if (roles.isEmpty() == false) {
            stringBuilder.append('{');
            roles.stream().map(DiscoveryNodeRole::roleNameAbbreviation).sorted().forEach(stringBuilder::append);
            stringBuilder.append('}');
        }
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

        builder.startArray("roles");
        for (DiscoveryNodeRole role : roles) {
            builder.value(role.roleName());
        }
        builder.endArray();

        builder.endObject();
        return builder;
    }

}
