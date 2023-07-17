/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.core.Assertions;
import org.elasticsearch.internal.VersionExtension;

import java.io.IOException;
import java.lang.reflect.Field;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import java.util.NavigableMap;
import java.util.Set;
import java.util.TreeMap;

/**
 * Represents the version of the wire protocol used to communicate between a pair of ES nodes.
 * <p>
 * Prior to 8.8.0, the release {@link Version} was used everywhere. This class separates the wire protocol version from the release version.
 * <p>
 * Each transport version constant has an id number, which for versions prior to 8.9.0 is the same as the release version for backwards
 * compatibility. In 8.9.0 this is changed to an incrementing number, disconnected from the release version.
 * <p>
 * Each version constant has a unique id string. This is not actually used in the binary protocol, but is there to ensure each protocol
 * version is only added to the source file once. This string needs to be unique (normally a UUID, but can be any other unique nonempty
 * string). If two concurrent PRs add the same transport version, the different unique ids cause a git conflict, ensuring that the second PR
 * to be merged must be updated with the next free version first. Without the unique id string, git will happily merge the two versions
 * together, resulting in the same transport version being used across multiple commits, causing problems when you try to upgrade between
 * those two merged commits.
 * <h2>Version compatibility</h2>
 * The earliest compatible version is hardcoded in the {@link #MINIMUM_COMPATIBLE} field. Previously, this was dynamically calculated from
 * the major/minor versions of {@link Version}, but {@code TransportVersion} does not have separate major/minor version numbers. So the
 * minimum compatible version is hard-coded as the transport version used by the highest minor release of the previous major version. {@link
 * #MINIMUM_COMPATIBLE} should be updated appropriately whenever a major release happens.
 * <p>
 * The earliest CCS compatible version is hardcoded at {@link #MINIMUM_CCS_VERSION}, as the transport version used by the previous minor
 * release. This should be updated appropriately whenever a minor release happens.
 * <h2>Adding a new version</h2>
 * A new transport version should be added <em>every time</em> a change is made to the serialization protocol of one or more classes. Each
 * transport version should only be used in a single merged commit (apart from BwC versions copied from {@link Version}).
 * <p>
 * To add a new transport version, add a new constant at the bottom of the list that is one greater than the current highest version, ensure
 * it has a unique id, and update the {@link CurrentHolder#CURRENT} constant to point to the new version.
 * <h2>Reverting a transport version</h2>
 * If you revert a commit with a transport version change, you <em>must</em> ensure there is a <em>new</em> transport version representing
 * the reverted change. <em>Do not</em> let the transport version go backwards, it must <em>always</em> be incremented.
 * <h2>Scope of usefulness of {@link TransportVersion}</h2>
 * {@link TransportVersion} is a property of the transport connection between a pair of nodes, and should not be used as an indication of
 * the version of any single node. The {@link TransportVersion} of a connection is negotiated between the nodes via some logic that is not
 * totally trivial, and may change in future. Any other places that might make decisions based on this version effectively have to reproduce
 * this negotiation logic, which would be fragile. If you need to make decisions based on the version of a single node, do so using a
 * different version value. If you need to know whether the cluster as a whole speaks a new enough {@link TransportVersion} to understand a
 * newly-added feature, use {@link org.elasticsearch.cluster.ClusterState#getMinTransportVersion}.
 */
public record TransportVersion(int id) implements Comparable<TransportVersion> {

    /*
     * NOTE: IntelliJ lies!
     * This map is used during class construction, referenced by the registerTransportVersion method.
     * When all the transport version constants have been registered, the map is cleared & never touched again.
     */
    private static Map<String, Integer> IDS = new HashMap<>();

    private static TransportVersion registerTransportVersion(int id, String uniqueId) {
        if (IDS == null) throw new IllegalStateException("The IDS map needs to be present to call this method");

        Strings.requireNonEmpty(uniqueId, "Each TransportVersion needs a unique string id");
        Integer existing = IDS.put(uniqueId, id);
        if (existing != null) {
            throw new IllegalArgumentException("Versions " + id + " and " + existing + " have the same unique id");
        }
        return new TransportVersion(id);
    }

    public static final TransportVersion ZERO = registerTransportVersion(0, "00000000-0000-0000-0000-000000000000");
    public static final TransportVersion V_7_0_0 = registerTransportVersion(7_00_00_99, "7505fd05-d982-43ce-a63f-ff4c6c8bdeec");
    public static final TransportVersion V_7_0_1 = registerTransportVersion(7_00_01_99, "ae772780-e6f9-46a1-b0a0-20ed0cae37f7");
    public static final TransportVersion V_7_1_0 = registerTransportVersion(7_01_00_99, "fd09007c-1c54-450a-af99-9f941e1a53c2");
    public static final TransportVersion V_7_2_0 = registerTransportVersion(7_02_00_99, "b74dbc52-e727-472c-af21-2156482e8796");
    public static final TransportVersion V_7_2_1 = registerTransportVersion(7_02_01_99, "a3217b94-f436-4aab-a020-162c83ba18f2");
    public static final TransportVersion V_7_3_0 = registerTransportVersion(7_03_00_99, "4f04e4c9-c5aa-49e4-8b99-abeb4e284a5a");
    public static final TransportVersion V_7_3_2 = registerTransportVersion(7_03_02_99, "60da3953-8415-4d4f-a18d-853c3e68ebd6");
    public static final TransportVersion V_7_4_0 = registerTransportVersion(7_04_00_99, "ec7e58aa-55b4-4064-a9dd-fd723a2ba7a8");
    public static final TransportVersion V_7_5_0 = registerTransportVersion(7_05_00_99, "cc6e14dc-9dc7-4b74-8e15-1f99a6cfbe03");
    public static final TransportVersion V_7_6_0 = registerTransportVersion(7_06_00_99, "4637b8ae-f3df-43ae-a065-ad4c29f3373a");
    public static final TransportVersion V_7_7_0 = registerTransportVersion(7_07_00_99, "7bb73c48-ddb8-4437-b184-30371c35dd4b");
    public static final TransportVersion V_7_8_0 = registerTransportVersion(7_08_00_99, "c3cc74af-d15e-494b-a907-6ad6dd2f4660");
    public static final TransportVersion V_7_8_1 = registerTransportVersion(7_08_01_99, "7acb9f6e-32f2-45ce-b87d-ca1f165b8e7a");
    public static final TransportVersion V_7_9_0 = registerTransportVersion(7_09_00_99, "9388fe76-192a-4053-b51c-d2a7b8eae545");
    public static final TransportVersion V_7_10_0 = registerTransportVersion(7_10_00_99, "4efca195-38e4-4f74-b877-c26fb2a40733");
    public static final TransportVersion V_7_10_1 = registerTransportVersion(7_10_01_99, "0070260c-aa0b-4fc2-9c87-5cd5f23b005f");
    public static final TransportVersion V_7_11_0 = registerTransportVersion(7_11_00_99, "3b43bcbc-1c5e-4cc2-a3b4-8ac8b64239e8");
    public static final TransportVersion V_7_12_0 = registerTransportVersion(7_12_00_99, "3be9ff6f-2d9f-4fc2-ba91-394dd5ebcf33");
    public static final TransportVersion V_7_13_0 = registerTransportVersion(7_13_00_99, "e1fe494a-7c66-4571-8f8f-1d7e6d8df1b3");
    public static final TransportVersion V_7_14_0 = registerTransportVersion(7_14_00_99, "8cf0954c-b085-467f-b20b-3cb4b2e69e3e");
    public static final TransportVersion V_7_15_0 = registerTransportVersion(7_15_00_99, "2273ac0e-00bb-4024-9e2e-ab78981623c6");
    public static final TransportVersion V_7_15_1 = registerTransportVersion(7_15_01_99, "a8c3503d-3452-45cf-b385-e855e16547fe");
    public static final TransportVersion V_7_16_0 = registerTransportVersion(7_16_00_99, "59abadd2-25db-4547-a991-c92306a3934e");
    public static final TransportVersion V_7_17_0 = registerTransportVersion(7_17_00_99, "322efe93-4c73-4e15-9274-bb76836c8fa8");
    public static final TransportVersion V_7_17_1 = registerTransportVersion(7_17_01_99, "51c72842-7974-4669-ad25-bf13ba307307");
    public static final TransportVersion V_7_17_8 = registerTransportVersion(7_17_08_99, "82a3e70d-cf0e-4efb-ad16-6077ab9fe19f");
    public static final TransportVersion V_8_0_0 = registerTransportVersion(8_00_00_99, "c7d2372c-9f01-4a79-8b11-227d862dfe4f");
    public static final TransportVersion V_8_1_0 = registerTransportVersion(8_01_00_99, "3dc49dce-9cef-492a-ac8d-3cc79f6b4280");
    public static final TransportVersion V_8_2_0 = registerTransportVersion(8_02_00_99, "8ce6d555-202e-47db-ab7d-ade9dda1b7e8");
    public static final TransportVersion V_8_3_0 = registerTransportVersion(8_03_00_99, "559ddb66-d857-4208-bed5-a995ccf478ea");
    public static final TransportVersion V_8_4_0 = registerTransportVersion(8_04_00_99, "c0d12906-aa5b-45d4-94c7-cbcf4d9818ca");
    public static final TransportVersion V_8_5_0 = registerTransportVersion(8_05_00_99, "be3d7f23-7240-4904-9d7f-e25a0f766eca");
    public static final TransportVersion V_8_6_0 = registerTransportVersion(8_06_00_99, "e209c5ed-3488-4415-b561-33492ca3b789");
    public static final TransportVersion V_8_6_1 = registerTransportVersion(8_06_01_99, "9f113acb-1b21-4fda-bef9-2a3e669b5c7b");
    public static final TransportVersion V_8_7_0 = registerTransportVersion(8_07_00_99, "f1ee7a85-4fa6-43f5-8679-33e2b750448b");
    public static final TransportVersion V_8_7_1 = registerTransportVersion(8_07_01_99, "018de9d8-9e8b-4ac7-8f4b-3a6fbd0487fb");
    public static final TransportVersion V_8_8_0 = registerTransportVersion(8_08_00_99, "f64fe576-0767-4ec3-984e-3e30b33b6c46");
    public static final TransportVersion V_8_8_1 = registerTransportVersion(8_08_01_99, "291c71bb-5b0a-4b7e-a407-6e53bc128d0f");

    /*
     * READ THE JAVADOC ABOVE BEFORE ADDING NEW TRANSPORT VERSIONS
     * Detached transport versions added below here.
     */
    public static final TransportVersion V_8_500_010 = registerTransportVersion(8_500_010, "9818C628-1EEC-439B-B943-468F61460675");
    public static final TransportVersion V_8_500_011 = registerTransportVersion(8_500_011, "2209F28D-B52E-4BC4-9889-E780F291C32E");
    public static final TransportVersion V_8_500_012 = registerTransportVersion(8_500_012, "BB6F4AF1-A860-4FD4-A138-8150FFBE0ABD");
    public static final TransportVersion V_8_500_013 = registerTransportVersion(8_500_013, "f65b85ac-db5e-4558-a487-a1dde4f6a33a");
    public static final TransportVersion V_8_500_014 = registerTransportVersion(8_500_014, "D115A2E1-1739-4A02-AB7B-64F6EA157EFB");
    public static final TransportVersion V_8_500_015 = registerTransportVersion(8_500_015, "651216c9-d54f-4189-9fe1-48d82d276863");
    public static final TransportVersion V_8_500_016 = registerTransportVersion(8_500_016, "492C94FB-AAEA-4C9E-8375-BDB67A398584");

    public static final TransportVersion V_8_500_017 = registerTransportVersion(8_500_017, "0EDCB5BA-049C-443C-8AB1-5FA58FB996FB");
    public static final TransportVersion V_8_500_018 = registerTransportVersion(8_500_018, "827C32CE-33D9-4AC3-A773-8FB768F59EAF");
    public static final TransportVersion V_8_500_019 = registerTransportVersion(8_500_019, "09bae57f-cab8-423c-aab3-c9778509ffe3");
    // 8.9.0
    public static final TransportVersion V_8_500_020 = registerTransportVersion(8_500_020, "ECB42C26-B258-42E5-A835-E31AF84A76DE");
    public static final TransportVersion V_8_500_021 = registerTransportVersion(8_500_021, "102e0d84-0c08-402c-a696-935f3a3da873");
    // Introduced for stateless plugin
    public static final TransportVersion V_8_500_022 = registerTransportVersion(8_500_022, "4993c724-7a81-4955-84e7-403484610091");
    public static final TransportVersion V_8_500_023 = registerTransportVersion(8_500_023, "01b06435-5d73-42ff-a121-3b36b771375e");
    public static final TransportVersion V_8_500_024 = registerTransportVersion(8_500_024, "db337007-f823-4dbd-968e-375383814c17");
    public static final TransportVersion V_8_500_025 = registerTransportVersion(8_500_025, "b2ab7b75-5ac2-4a3b-bbb6-8789ca66722d");
    public static final TransportVersion V_8_500_026 = registerTransportVersion(8_500_026, "965d294b-14aa-4abb-bcfc-34631187941d");
    public static final TransportVersion V_8_500_027 = registerTransportVersion(8_500_027, "B151D967-8E7C-401C-8275-0ABC06335F2D");
    public static final TransportVersion V_8_500_028 = registerTransportVersion(8_500_028, "a6592d08-15cb-4e1a-b9b4-b2ba24058444");
    public static final TransportVersion V_8_500_029 = registerTransportVersion(8_500_029, "f3bd98af-6187-e161-e315-718a2fecc2db");
    public static final TransportVersion V_8_500_030 = registerTransportVersion(8_500_030, "b72d7f12-8ed3-4a5b-8e6a-4910ea10e0d7");
    public static final TransportVersion V_8_500_031 = registerTransportVersion(8_500_031, "e7aa7e95-37e7-46a3-aad1-90a21c0769e7");
    public static final TransportVersion V_8_500_032 = registerTransportVersion(8_500_032, "a9a14bc6-c3f2-41d9-a3d8-c686bf2c901d");
    public static final TransportVersion V_8_500_033 = registerTransportVersion(8_500_033, "193ab7c4-a751-4cbd-a66a-2d7d56ccbc10");
    public static final TransportVersion V_8_500_034 = registerTransportVersion(8_500_034, "16871c8b-88ba-4432-980a-10fd9ecad2dc");
    public static final TransportVersion V_8_500_035 = registerTransportVersion(8_500_035, "664dd6ce-3487-4fbd-81a9-af778b28be45");

    // Introduced for stateless plugin
    public static final TransportVersion V_8_500_036 = registerTransportVersion(8_500_036, "3343c64f-d7ac-4f02-9262-3e1acfc56f89");

    private static class CurrentHolder {
        private static final TransportVersion CURRENT = findCurrent(V_8_500_036);

        // finds the pluggable current version, or uses the given fallback
        private static TransportVersion findCurrent(TransportVersion fallback) {
            var versionExtension = VersionExtension.load();
            if (versionExtension == null) {
                return fallback;
            }
            var version = versionExtension.getCurrentTransportVersion();
            assert version.onOrAfter(fallback);
            return version;
        }
    }

    /**
     * Reference to the earliest compatible transport version to this version of the codebase.
     * This should be the transport version used by the highest minor version of the previous major.
     */
    public static final TransportVersion MINIMUM_COMPATIBLE = V_7_17_0;

    /**
     * Reference to the minimum transport version that can be used with CCS.
     * This should be the transport version used by the previous minor release.
     */
    public static final TransportVersion MINIMUM_CCS_VERSION = V_8_500_020;

    static {
        // see comment on IDS field
        // now we're registered all the transport versions, we can clear the map
        IDS = null;
    }

    static NavigableMap<Integer, TransportVersion> getAllVersionIds(Class<?> cls) {
        Map<Integer, String> versionIdFields = new HashMap<>();
        NavigableMap<Integer, TransportVersion> builder = new TreeMap<>();

        Set<String> ignore = Set.of("ZERO", "CURRENT", "MINIMUM_COMPATIBLE", "MINIMUM_CCS_VERSION");

        for (Field declaredField : cls.getFields()) {
            if (declaredField.getType().equals(TransportVersion.class)) {
                String fieldName = declaredField.getName();
                if (ignore.contains(fieldName)) {
                    continue;
                }

                TransportVersion version;
                try {
                    version = (TransportVersion) declaredField.get(null);
                } catch (IllegalAccessException e) {
                    throw new AssertionError(e);
                }
                builder.put(version.id, version);

                if (Assertions.ENABLED) {
                    // check the version number is unique
                    var sameVersionNumber = versionIdFields.put(version.id, fieldName);
                    assert sameVersionNumber == null
                        : "Versions ["
                            + sameVersionNumber
                            + "] and ["
                            + fieldName
                            + "] have the same version number ["
                            + version.id
                            + "]. Each TransportVersion should have a different version number";
                }
            }
        }

        return Collections.unmodifiableNavigableMap(builder);
    }

    private static final NavigableMap<Integer, TransportVersion> VERSION_IDS = getAllVersionIds(TransportVersion.class);

    static Collection<TransportVersion> getAllVersions() {
        return VERSION_IDS.values();
    }

    public static TransportVersion readVersion(StreamInput in) throws IOException {
        return fromId(in.readVInt());
    }

    public static TransportVersion fromId(int id) {
        TransportVersion known = VERSION_IDS.get(id);
        if (known != null) {
            return known;
        }
        // this is a version we don't otherwise know about - just create a placeholder
        return new TransportVersion(id);
    }

    public static void writeVersion(TransportVersion version, StreamOutput out) throws IOException {
        out.writeVInt(version.id);
    }

    /**
     * Returns the minimum version of {@code version1} and {@code version2}
     */
    public static TransportVersion min(TransportVersion version1, TransportVersion version2) {
        return version1.id < version2.id ? version1 : version2;
    }

    /**
     * Returns the maximum version of {@code version1} and {@code version2}
     */
    public static TransportVersion max(TransportVersion version1, TransportVersion version2) {
        return version1.id > version2.id ? version1 : version2;
    }

    /**
     * Returns {@code true} if the specified version is compatible with this running version of Elasticsearch.
     */
    public static boolean isCompatible(TransportVersion version) {
        return version.onOrAfter(MINIMUM_COMPATIBLE);
    }

    /**
     * Reference to the most recent transport version.
     * This should be the transport version with the highest id.
     */
    public static TransportVersion current() {
        return CurrentHolder.CURRENT;
    }

    public boolean after(TransportVersion version) {
        return version.id < id;
    }

    public boolean onOrAfter(TransportVersion version) {
        return version.id <= id;
    }

    public boolean before(TransportVersion version) {
        return version.id > id;
    }

    public boolean onOrBefore(TransportVersion version) {
        return version.id >= id;
    }

    public boolean between(TransportVersion lowerInclusive, TransportVersion upperExclusive) {
        if (upperExclusive.onOrBefore(lowerInclusive)) throw new IllegalArgumentException();
        return onOrAfter(lowerInclusive) && before(upperExclusive);
    }

    public static TransportVersion fromString(String str) {
        return TransportVersion.fromId(Integer.parseInt(str));
    }

    @Override
    public int compareTo(TransportVersion other) {
        return Integer.compare(this.id, other.id);
    }

    @Override
    public String toString() {
        return Integer.toString(id);
    }
}
