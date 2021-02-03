/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ingest;

import org.elasticsearch.ingest.AbstractProcessor;
import org.elasticsearch.ingest.ConfigurationUtils;
import org.elasticsearch.ingest.IngestDocument;
import org.elasticsearch.ingest.Processor;
import org.elasticsearch.common.network.InetAddresses;
import org.elasticsearch.xpack.core.common.network.CIDRUtils;

import java.net.InetAddress;
import java.util.List;
import java.util.Map;
import java.util.Arrays;

import static org.elasticsearch.ingest.ConfigurationUtils.readBooleanProperty;

public class NetworkDirectionProcessor extends AbstractProcessor {
    static final byte[] UNDEFINED_IP4 = new byte[] { 0, 0, 0, 0 };
    static final byte[] UNDEFINED_IP6 = new byte[] { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 };
    static final byte[] BROADCAST_IP4 = new byte[] { (byte) 0xff, (byte) 0xff, (byte) 0xff, (byte) 0xff };

    public static final String TYPE = "network_direction";

    public static final String DIRECTION_INTERNAL = "internal";
    public static final String DIRECTION_EXTERNAL = "external";
    public static final String DIRECTION_INBOUND = "inbound";
    public static final String DIRECTION_OUTBOUND = "outbound";

    private static final String LOOPBACK_NAMED_NETWORK = "loopback";
    private static final String GLOBAL_UNICAST_NAMED_NETWORK = "global_unicast";
    private static final String UNICAST_NAMED_NETWORK = "unicast";
    private static final String LINK_LOCAL_UNICAST_NAMED_NETWORK = "link_local_unicast";
    private static final String INTERFACE_LOCAL_NAMED_NETWORK = "interface_local_multicast";
    private static final String LINK_LOCAL_MULTICAST_NAMED_NETWORK = "link_local_multicast";
    private static final String MULTICAST_NAMED_NETWORK = "multicast";
    private static final String UNSPECIFIED_NAMED_NETWORK = "unspecified";
    private static final String PRIVATE_NAMED_NETWORK = "private";
    private static final String PUBLIC_NAMED_NETWORK = "public";

    private final String sourceIpField;
    private final String destinationIpField;
    private final String targetField;
    private final List<String> internalNetworks;
    private final boolean ignoreMissing;

    NetworkDirectionProcessor(
        String tag,
        String description,
        String sourceIpField,
        String destinationIpField,
        String targetField,
        List<String> internalNetworks,
        boolean ignoreMissing
    ) {
        super(tag, description);
        this.sourceIpField = sourceIpField;
        this.destinationIpField = destinationIpField;
        this.targetField = targetField;
        this.internalNetworks = internalNetworks;
        this.ignoreMissing = ignoreMissing;
    }

    public String getSourceIpField() {
        return sourceIpField;
    }

    public String getDestinationIpField() {
        return destinationIpField;
    }

    public String getTargetField() {
        return targetField;
    }

    public List<String> getInternalNetworks() {
        return internalNetworks;
    }

    public boolean getIgnoreMissing() {
        return ignoreMissing;
    }

    @Override
    public IngestDocument execute(IngestDocument ingestDocument) throws Exception {
        String direction = getDirection(ingestDocument);
        if (direction == null) {
            if (ignoreMissing) {
                return ingestDocument;
            } else {
                throw new IllegalArgumentException("unable to calculate network direction from document");
            }
        }

        ingestDocument.setFieldValue(targetField, direction);
        return ingestDocument;
    }

    private String getDirection(IngestDocument d) {
        if (internalNetworks == null) {
            return null;
        }

        String sourceIpAddrString = d.getFieldValue(sourceIpField, String.class, ignoreMissing);
        if (sourceIpAddrString == null) {
            return null;
        }

        String destIpAddrString = d.getFieldValue(destinationIpField, String.class, ignoreMissing);
        if (destIpAddrString == null) {
            return null;
        }

        boolean sourceInternal = isInternal(sourceIpAddrString);
        boolean destinationInternal = isInternal(destIpAddrString);

        if (sourceInternal && destinationInternal) {
            return DIRECTION_INTERNAL;
        }
        if (sourceInternal) {
            return DIRECTION_OUTBOUND;
        }
        if (destinationInternal) {
            return DIRECTION_INBOUND;
        }
        return DIRECTION_EXTERNAL;
    }

    private boolean isInternal(String ip) {
        for (String network : internalNetworks) {
            if (inNetwork(ip, network)) {
                return true;
            }
        }
        return false;
    }

    private boolean inNetwork(String ip, String network) {
        InetAddress address = InetAddresses.forString(ip);
        switch (network) {
            case LOOPBACK_NAMED_NETWORK:
                return isLoopback(address);
            case GLOBAL_UNICAST_NAMED_NETWORK:
            case UNICAST_NAMED_NETWORK:
                return isUnicast(address);
            case LINK_LOCAL_UNICAST_NAMED_NETWORK:
                return isLinkLocalUnicast(address);
            case INTERFACE_LOCAL_NAMED_NETWORK:
                return isInterfaceLocalMulticast(address);
            case LINK_LOCAL_MULTICAST_NAMED_NETWORK:
                return isLinkLocalMulticast(address);
            case MULTICAST_NAMED_NETWORK:
                return isMulticast(address);
            case UNSPECIFIED_NAMED_NETWORK:
                return isUnspecified(address);
            case PRIVATE_NAMED_NETWORK:
                return isPrivate(ip);
            case PUBLIC_NAMED_NETWORK:
                return isPublic(ip);
            default:
                return CIDRUtils.isInRange(ip, network);
        }
    }

    private boolean isLoopback(InetAddress ip) {
        return ip.isLoopbackAddress();
    }

    private boolean isUnicast(InetAddress ip) {
        return Arrays.equals(ip.getAddress(), BROADCAST_IP4) == false
            && isUnspecified(ip) == false
            && isLoopback(ip) == false
            && isMulticast(ip) == false
            && isLinkLocalUnicast(ip) == false;
    }

    private boolean isLinkLocalUnicast(InetAddress ip) {
        return ip.isLinkLocalAddress();
    }

    private boolean isInterfaceLocalMulticast(InetAddress ip) {
        return ip.isMCNodeLocal();
    }

    private boolean isLinkLocalMulticast(InetAddress ip) {
        return ip.isMCLinkLocal();
    }

    private boolean isMulticast(InetAddress ip) {
        return ip.isMulticastAddress();
    }

    private boolean isUnspecified(InetAddress ip) {
        var address = ip.getAddress();
        return Arrays.equals(UNDEFINED_IP4, address) || Arrays.equals(UNDEFINED_IP6, address);
    }

    private boolean isPrivate(String ip) {
        return CIDRUtils.isInRange(ip, "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fd00::/8");
    }

    private boolean isPublic(String ip) {
        return isLocalOrPrivate(ip) == false;
    }

    private boolean isLocalOrPrivate(String ip) {
        var address = InetAddresses.forString(ip);
        return isPrivate(ip)
            || isLoopback(address)
            || isUnspecified(address)
            || isLinkLocalUnicast(address)
            || isLinkLocalMulticast(address)
            || isInterfaceLocalMulticast(address)
            || Arrays.equals(address.getAddress(), BROADCAST_IP4);
    }

    @Override
    public String getType() {
        return TYPE;
    }

    public static final class Factory implements Processor.Factory {

        static final String DEFAULT_SOURCE_IP = "source.ip";
        static final String DEFAULT_DEST_IP = "destination.ip";
        static final String DEFAULT_TARGET = "network.direction";

        @Override
        public NetworkDirectionProcessor create(
            Map<String, Processor.Factory> registry,
            String processorTag,
            String description,
            Map<String, Object> config
        ) throws Exception {
            String sourceIpField = ConfigurationUtils.readStringProperty(TYPE, processorTag, config, "source_ip", DEFAULT_SOURCE_IP);
            String destIpField = ConfigurationUtils.readStringProperty(TYPE, processorTag, config, "destination_ip", DEFAULT_DEST_IP);
            String targetField = ConfigurationUtils.readStringProperty(TYPE, processorTag, config, "target_field", DEFAULT_TARGET);
            List<String> internalNetworks = ConfigurationUtils.readList(TYPE, processorTag, config, "internal_networks");
            boolean ignoreMissing = readBooleanProperty(TYPE, processorTag, config, "ignore_missing", true);

            return new NetworkDirectionProcessor(
                processorTag,
                description,
                sourceIpField,
                destIpField,
                targetField,
                internalNetworks,
                ignoreMissing
            );
        }
    }
}
