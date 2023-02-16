/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.test;

import org.elasticsearch.TransportVersion;
import org.elasticsearch.core.Nullable;

import java.util.Collections;
import java.util.List;
import java.util.Optional;
import java.util.Random;
import java.util.Set;
import java.util.stream.Collectors;

import static org.elasticsearch.KnownTransportVersions.ALL_VERSIONS;

public class TransportVersionUtils {
    /** Returns all released versions */
    public static List<TransportVersion> allReleasedVersions() {
        return ALL_VERSIONS;
    }

    /** Returns the oldest known {@link TransportVersion} */
    public static TransportVersion getFirstVersion() {
        return ALL_VERSIONS.get(0);
    }

    /** Returns a random {@link TransportVersion} from all available versions. */
    public static TransportVersion randomVersion() {
        return ESTestCase.randomFrom(ALL_VERSIONS);
    }

    /** Returns a random {@link TransportVersion} from all available versions without the ignore set */
    public static TransportVersion randomVersion(Set<TransportVersion> ignore) {
        return ESTestCase.randomFrom(ALL_VERSIONS.stream().filter(v -> ignore.contains(v) == false).collect(Collectors.toList()));
    }

    /** Returns a random {@link TransportVersion} from all available versions. */
    public static TransportVersion randomVersion(Random random) {
        return ALL_VERSIONS.get(random.nextInt(ALL_VERSIONS.size()));
    }

    /** Returns a random {@link TransportVersion} between <code>minVersion</code> and <code>maxVersion</code> (inclusive). */
    public static TransportVersion randomVersionBetween(
        Random random,
        @Nullable TransportVersion minVersion,
        @Nullable TransportVersion maxVersion
    ) {
        int minVersionIndex = 0;
        if (minVersion != null) {
            minVersionIndex = ALL_VERSIONS.indexOf(minVersion);
        }
        int maxVersionIndex = ALL_VERSIONS.size() - 1;
        if (maxVersion != null) {
            maxVersionIndex = ALL_VERSIONS.indexOf(maxVersion);
        }
        if (minVersionIndex == -1) {
            throw new IllegalArgumentException("minVersion [" + minVersion + "] does not exist.");
        } else if (maxVersionIndex == -1) {
            throw new IllegalArgumentException("maxVersion [" + maxVersion + "] does not exist.");
        } else if (minVersionIndex > maxVersionIndex) {
            throw new IllegalArgumentException("maxVersion [" + maxVersion + "] cannot be less than minVersion [" + minVersion + "]");
        } else {
            // minVersionIndex is inclusive so need to add 1 to this index
            int range = maxVersionIndex + 1 - minVersionIndex;
            return ALL_VERSIONS.get(minVersionIndex + random.nextInt(range));
        }
    }

    public static TransportVersion getPreviousVersion() {
        TransportVersion version = getPreviousVersion(TransportVersion.CURRENT);
        assert version.before(TransportVersion.CURRENT);
        return version;
    }

    public static TransportVersion getPreviousVersion(TransportVersion version) {
        int place = Collections.binarySearch(ALL_VERSIONS, version);
        if (place < 0) {
            // version does not exist - need the item before the index this version should be inserted
            place = -(place + 1);
        }

        if (place < 1) {
            throw new IllegalArgumentException("couldn't find any released versions before [" + version + "]");
        }
        return ALL_VERSIONS.get(place - 1);
    }

    public static TransportVersion getNextVersion(TransportVersion version) {
        int place = Collections.binarySearch(ALL_VERSIONS, version);
        if (place < 0) {
            // version does not exist - need the item at the index this version should be inserted
            place = -(place + 1);
        } else {
            // need the *next* version
            place++;
        }

        if (place < 0 || place >= ALL_VERSIONS.size()) {
            throw new IllegalArgumentException("couldn't find any released versions after [" + version + "]");
        }
        return ALL_VERSIONS.get(place);
    }

    public static boolean isCompatible(TransportVersion version1, TransportVersion version2) {
        return version1.onOrAfter(minimumCompatibilityVersion(version2)) && version2.onOrAfter(minimumCompatibilityVersion(version1));
    }

    public static TransportVersion minimumCompatibilityVersion(TransportVersion version) {
        return VersionUtils.findVersion(version).minimumCompatibilityVersion().transportVersion;
    }

    /** Returns a random {@link TransportVersion} from all available versions, that is compatible with the given version. */
    public static TransportVersion randomCompatibleVersion(Random random, TransportVersion version) {
        final List<TransportVersion> compatible = ALL_VERSIONS.stream().filter(v -> isCompatible(v, version)).toList();
        return compatible.get(random.nextInt(compatible.size()));
    }

    public static TransportVersion randomPreviousCompatibleVersion(Random random, TransportVersion version) {
        return randomVersionBetween(random, minimumCompatibilityVersion(version), getPreviousVersion(version));
    }

    /** returns the first future compatible version */
    public static TransportVersion compatibleFutureVersion(TransportVersion version) {
        final Optional<TransportVersion> opt = ALL_VERSIONS.stream()
            .filter(version::before)
            .filter(v -> isCompatible(v, version))
            .findAny();
        assert opt.isPresent() : "no future compatible version for " + version;
        return opt.get();
    }
}
