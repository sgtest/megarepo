/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.profiling;

import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.common.collect.Iterators;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ChunkedToXContentHelper;
import org.elasticsearch.common.xcontent.ChunkedToXContentObject;
import org.elasticsearch.xcontent.ToXContent;

import java.io.IOException;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

public class GetFlamegraphResponse extends ActionResponse implements ChunkedToXContentObject {
    private final int size;
    private final double samplingRate;
    private final List<Map<String, Integer>> edges;
    private final List<String> fileIds;
    private final List<Integer> frameTypes;
    private final List<Boolean> inlineFrames;
    private final List<String> fileNames;
    private final List<Integer> addressOrLines;
    private final List<String> functionNames;
    private final List<Integer> functionOffsets;
    private final List<String> sourceFileNames;
    private final List<Integer> sourceLines;
    private final List<Integer> countInclusive;
    private final List<Integer> countExclusive;

    public GetFlamegraphResponse(StreamInput in) throws IOException {
        this.size = in.readInt();
        this.samplingRate = in.readDouble();
        this.edges = in.readCollectionAsList(i -> i.readMap(StreamInput::readInt));
        this.fileIds = in.readCollectionAsList(StreamInput::readString);
        this.frameTypes = in.readCollectionAsList(StreamInput::readInt);
        this.inlineFrames = in.readCollectionAsList(StreamInput::readBoolean);
        this.fileNames = in.readCollectionAsList(StreamInput::readString);
        this.addressOrLines = in.readCollectionAsList(StreamInput::readInt);
        this.functionNames = in.readCollectionAsList(StreamInput::readString);
        this.functionOffsets = in.readCollectionAsList(StreamInput::readInt);
        this.sourceFileNames = in.readCollectionAsList(StreamInput::readString);
        this.sourceLines = in.readCollectionAsList(StreamInput::readInt);
        this.countInclusive = in.readCollectionAsList(StreamInput::readInt);
        this.countExclusive = in.readCollectionAsList(StreamInput::readInt);
    }

    public GetFlamegraphResponse(
        int size,
        double samplingRate,
        List<Map<String, Integer>> edges,
        List<String> fileIds,
        List<Integer> frameTypes,
        List<Boolean> inlineFrames,
        List<String> fileNames,
        List<Integer> addressOrLines,
        List<String> functionNames,
        List<Integer> functionOffsets,
        List<String> sourceFileNames,
        List<Integer> sourceLines,
        List<Integer> countInclusive,
        List<Integer> countExclusive
    ) {
        this.size = size;
        this.samplingRate = samplingRate;
        this.edges = edges;
        this.fileIds = fileIds;
        this.frameTypes = frameTypes;
        this.inlineFrames = inlineFrames;
        this.fileNames = fileNames;
        this.addressOrLines = addressOrLines;
        this.functionNames = functionNames;
        this.functionOffsets = functionOffsets;
        this.sourceFileNames = sourceFileNames;
        this.sourceLines = sourceLines;
        this.countInclusive = countInclusive;
        this.countExclusive = countExclusive;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeInt(this.size);
        out.writeDouble(this.samplingRate);
        out.writeCollection(this.edges, (o, v) -> o.writeMap(v, StreamOutput::writeString, StreamOutput::writeInt));
        out.writeCollection(this.fileIds, StreamOutput::writeString);
        out.writeCollection(this.frameTypes, StreamOutput::writeInt);
        out.writeCollection(this.inlineFrames, StreamOutput::writeBoolean);
        out.writeCollection(this.fileNames, StreamOutput::writeString);
        out.writeCollection(this.addressOrLines, StreamOutput::writeInt);
        out.writeCollection(this.functionNames, StreamOutput::writeString);
        out.writeCollection(this.functionOffsets, StreamOutput::writeInt);
        out.writeCollection(this.sourceFileNames, StreamOutput::writeString);
        out.writeCollection(this.sourceLines, StreamOutput::writeInt);
        out.writeCollection(this.countInclusive, StreamOutput::writeInt);
        out.writeCollection(this.countExclusive, StreamOutput::writeInt);
    }

    public int getSize() {
        return size;
    }

    public double getSamplingRate() {
        return samplingRate;
    }

    public List<Integer> getCountInclusive() {
        return countInclusive;
    }

    public List<Integer> getCountExclusive() {
        return countExclusive;
    }

    public List<Map<String, Integer>> getEdges() {
        return edges;
    }

    public List<String> getFileIds() {
        return fileIds;
    }

    public List<Integer> getFrameTypes() {
        return frameTypes;
    }

    public List<Boolean> getInlineFrames() {
        return inlineFrames;
    }

    public List<String> getFileNames() {
        return fileNames;
    }

    public List<Integer> getAddressOrLines() {
        return addressOrLines;
    }

    public List<String> getFunctionNames() {
        return functionNames;
    }

    public List<Integer> getFunctionOffsets() {
        return functionOffsets;
    }

    public List<String> getSourceFileNames() {
        return sourceFileNames;
    }

    public List<Integer> getSourceLines() {
        return sourceLines;
    }

    @Override
    public Iterator<? extends ToXContent> toXContentChunked(ToXContent.Params params) {
        return Iterators.concat(
            ChunkedToXContentHelper.startObject(),
            ChunkedToXContentHelper.array(
                "Edges",
                Iterators.flatMap(
                    edges.iterator(),
                    perNodeEdges -> Iterators.concat(
                        ChunkedToXContentHelper.startArray(),
                        Iterators.map(perNodeEdges.entrySet().iterator(), edge -> (b, p) -> b.value(edge.getValue())),
                        ChunkedToXContentHelper.endArray()
                    )
                )
            ),
            ChunkedToXContentHelper.array("FileID", Iterators.map(fileIds.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("FrameType", Iterators.map(frameTypes.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("Inline", Iterators.map(inlineFrames.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("ExeFilename", Iterators.map(fileNames.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("AddressOrLine", Iterators.map(addressOrLines.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("FunctionName", Iterators.map(functionNames.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("FunctionOffset", Iterators.map(functionOffsets.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("SourceFilename", Iterators.map(sourceFileNames.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("SourceLine", Iterators.map(sourceLines.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("CountInclusive", Iterators.map(countInclusive.iterator(), e -> (b, p) -> b.value(e))),
            ChunkedToXContentHelper.array("CountExclusive", Iterators.map(countExclusive.iterator(), e -> (b, p) -> b.value(e))),
            Iterators.single((b, p) -> b.field("Size", size)),
            Iterators.single((b, p) -> b.field("SamplingRate", samplingRate)),
            ChunkedToXContentHelper.endObject()
        );
    }
}
