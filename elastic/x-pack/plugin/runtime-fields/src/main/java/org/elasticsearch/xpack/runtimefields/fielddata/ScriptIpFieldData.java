/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.runtimefields.fielddata;

import org.apache.lucene.document.InetAddressPoint;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.network.InetAddresses;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.IndexFieldDataCache;
import org.elasticsearch.index.fielddata.ScriptDocValues;
import org.elasticsearch.index.fielddata.SortedBinaryDocValues;
import org.elasticsearch.index.mapper.IpFieldMapper;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.indices.breaker.CircuitBreakerService;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.aggregations.support.ValuesSourceType;
import org.elasticsearch.xpack.runtimefields.IpScriptFieldScript;

import java.net.InetAddress;

public class ScriptIpFieldData extends ScriptBinaryFieldData {
    public static class Builder implements IndexFieldData.Builder {
        private final String name;
        private final IpScriptFieldScript.LeafFactory leafFactory;

        public Builder(String name, IpScriptFieldScript.LeafFactory leafFactory) {
            this.name = name;
            this.leafFactory = leafFactory;
        }

        @Override
        public ScriptIpFieldData build(IndexFieldDataCache cache, CircuitBreakerService breakerService, MapperService mapperService) {
            return new ScriptIpFieldData(name, leafFactory);
        }
    }

    private final IpScriptFieldScript.LeafFactory leafFactory;

    private ScriptIpFieldData(String fieldName, IpScriptFieldScript.LeafFactory leafFactory) {
        super(fieldName);
        this.leafFactory = leafFactory;
    }

    @Override
    public ScriptBinaryLeafFieldData loadDirect(LeafReaderContext context) throws Exception {
        IpScriptFieldScript script = leafFactory.newInstance(context);
        return new ScriptBinaryLeafFieldData() {
            @Override
            public ScriptDocValues<String> getScriptValues() {
                return new IpScriptDocValues(getBytesValues());
            }

            @Override
            public SortedBinaryDocValues getBytesValues() {
                return new ScriptIpDocValues(script);
            }
        };
    }

    @Override
    public ValuesSourceType getValuesSourceType() {
        return CoreValuesSourceType.IP;
    }

    /**
     * Doc values implementation for ips. We can't share
     * {@link IpFieldMapper.IpFieldType.IpScriptDocValues} because it is based
     * on global ordinals and we don't have those.
     */
    public static class IpScriptDocValues extends ScriptDocValues.Strings {
        public IpScriptDocValues(SortedBinaryDocValues in) {
            super(in);
        }

        @Override
        protected String bytesToString(BytesRef bytes) {
            InetAddress addr = InetAddressPoint.decode(BytesReference.toBytes(new BytesArray(bytes)));
            return InetAddresses.toAddrString(addr);
        }
    }
}
