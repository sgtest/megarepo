/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.rest.root;

import org.elasticsearch.Build;
import org.elasticsearch.TransportVersion;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.index.IndexVersion;
import org.elasticsearch.xcontent.ObjectParser;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xcontent.ToXContentObject;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentParser;

import java.io.IOException;
import java.util.Objects;

public class MainResponse extends ActionResponse implements ToXContentObject {

    private String nodeName;
    private Version version;
    private IndexVersion indexVersion;
    private TransportVersion transportVersion;
    private ClusterName clusterName;
    private String clusterUuid;
    private Build build;

    MainResponse() {}

    MainResponse(StreamInput in) throws IOException {
        super(in);
        nodeName = in.readString();
        version = Version.readVersion(in);
        indexVersion = in.getTransportVersion().onOrAfter(TransportVersion.V_8_500_031) ? IndexVersion.readVersion(in) : null;
        transportVersion = in.getTransportVersion().onOrAfter(TransportVersion.V_8_500_019) ? TransportVersion.readVersion(in) : null;
        clusterName = new ClusterName(in);
        clusterUuid = in.readString();
        build = Build.readBuild(in);
    }

    public MainResponse(
        String nodeName,
        Version version,
        IndexVersion indexVersion,
        TransportVersion transportVersion,
        ClusterName clusterName,
        String clusterUuid,
        Build build
    ) {
        this.nodeName = nodeName;
        this.version = version;
        this.indexVersion = indexVersion;
        this.transportVersion = transportVersion;
        this.clusterName = clusterName;
        this.clusterUuid = clusterUuid;
        this.build = build;
    }

    public String getNodeName() {
        return nodeName;
    }

    public Version getVersion() {
        return version;
    }

    public IndexVersion getIndexVersion() {
        return indexVersion;
    }

    public TransportVersion getTransportVersion() {
        return transportVersion;
    }

    public ClusterName getClusterName() {
        return clusterName;
    }

    public String getClusterUuid() {
        return clusterUuid;
    }

    public Build getBuild() {
        return build;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeString(nodeName);
        Version.writeVersion(version, out);
        if (out.getTransportVersion().onOrAfter(TransportVersion.V_8_500_031)) {
            IndexVersion.writeVersion(indexVersion, out);
        }
        if (out.getTransportVersion().onOrAfter(TransportVersion.V_8_500_019)) {
            TransportVersion.writeVersion(transportVersion, out);
        }
        clusterName.writeTo(out);
        out.writeString(clusterUuid);
        Build.writeBuild(build, out);
    }

    private String getLuceneVersion() {
        if (indexVersion != null) {
            return indexVersion.luceneVersion().toString();
        } else if (version.before(Version.V_8_10_0)) {
            return IndexVersion.fromId(version.id).luceneVersion().toString();
        } else {
            return "unknown";
        }
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field("name", nodeName);
        builder.field("cluster_name", clusterName.value());
        builder.field("cluster_uuid", clusterUuid);
        builder.startObject("version")
            .field("number", build.qualifiedVersion())
            .field("build_flavor", "default")
            .field("build_type", build.type().displayName())
            .field("build_hash", build.hash())
            .field("build_date", build.date())
            .field("build_snapshot", build.isSnapshot())
            .field("lucene_version", getLuceneVersion())
            .field("index_version", indexVersion != null ? indexVersion.toString() : "unknown")
            .field("minimum_wire_compatibility_version", version.minimumCompatibilityVersion().toString())
            .field("minimum_index_compatibility_version", version.minimumIndexCompatibilityVersion().toString())
            .field("transport_version", transportVersion != null ? transportVersion.toString() : "unknown")
            .endObject();
        builder.field("tagline", "You Know, for Search");
        builder.endObject();
        return builder;
    }

    private static final ObjectParser<MainResponse, Void> PARSER = new ObjectParser<>(
        MainResponse.class.getName(),
        true,
        MainResponse::new
    );

    static {
        PARSER.declareString((response, value) -> response.nodeName = value, new ParseField("name"));
        PARSER.declareString((response, value) -> response.clusterName = new ClusterName(value), new ParseField("cluster_name"));
        PARSER.declareString((response, value) -> response.clusterUuid = value, new ParseField("cluster_uuid"));
        PARSER.declareString((response, value) -> {}, new ParseField("tagline"));
        PARSER.declareObject((response, value) -> {
            final String buildType = (String) value.get("build_type");
            response.build = new Build(
                /*
                 * Be lenient when reading on the wire, the enumeration values from other versions might be different than what
                 * we know.
                 */
                buildType == null ? Build.Type.UNKNOWN : Build.Type.fromDisplayName(buildType, false),
                (String) value.get("build_hash"),
                (String) value.get("build_date"),
                (boolean) value.get("build_snapshot"),
                (String) value.get("number")
            );
            response.version = Version.fromString(
                ((String) value.get("number")).replace("-SNAPSHOT", "").replaceFirst("-(alpha\\d+|beta\\d+|rc\\d+)", "")
            );

            String indexVersion = (String) value.get("index_version");
            response.indexVersion = indexVersion != null && indexVersion.equals("unknown") == false
                ? IndexVersion.fromId(Integer.parseInt(indexVersion))
                : null;

            String transportVersion = (String) value.get("transport_version");
            response.transportVersion = transportVersion != null && transportVersion.equals("unknown") == false
                ? TransportVersion.fromString(transportVersion)
                : null;
        }, (parser, context) -> parser.map(), new ParseField("version"));
    }

    public static MainResponse fromXContent(XContentParser parser) {
        return PARSER.apply(parser, null);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        MainResponse other = (MainResponse) o;
        return Objects.equals(nodeName, other.nodeName)
            && Objects.equals(version, other.version)
            && Objects.equals(indexVersion, other.indexVersion)
            && Objects.equals(transportVersion, other.transportVersion)
            && Objects.equals(clusterUuid, other.clusterUuid)
            && Objects.equals(build, other.build)
            && Objects.equals(clusterName, other.clusterName);
    }

    @Override
    public int hashCode() {
        return Objects.hash(nodeName, version, indexVersion, transportVersion, clusterUuid, build, clusterName);
    }

    @Override
    public String toString() {
        return "MainResponse{"
            + "nodeName='"
            + nodeName
            + '\''
            + ", version="
            + version
            + ", indexVersion="
            + indexVersion
            + ", transportVersion="
            + transportVersion
            + ", clusterName="
            + clusterName
            + ", clusterUuid='"
            + clusterUuid
            + '\''
            + ", build="
            + build
            + '}';
    }
}
