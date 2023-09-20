/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.profiling;

import java.util.List;

public class GetStackTracesActionIT extends ProfilingTestCase {
    public void testGetStackTracesUnfiltered() throws Exception {
        GetStackTracesRequest request = new GetStackTracesRequest(1, null);
        GetStackTracesResponse response = client().execute(GetStackTracesAction.INSTANCE, request).get();
        assertEquals(1, response.getTotalFrames());
        assertNotNull(response.getStackTraces());
        StackTrace stackTrace = response.getStackTraces().get("QjoLteG7HX3VUUXr-J4kHQ");
        assertEquals(List.of(1083999), stackTrace.addressOrLines);
        assertEquals(List.of("QCCDqjSg3bMK1C4YRK6Tiw"), stackTrace.fileIds);
        assertEquals(List.of("QCCDqjSg3bMK1C4YRK6TiwAAAAAAEIpf"), stackTrace.frameIds);
        assertEquals(List.of(2), stackTrace.typeIds);

        assertNotNull(response.getStackFrames());
        StackFrame stackFrame = response.getStackFrames().get("QCCDqjSg3bMK1C4YRK6TiwAAAAAAEIpf");
        assertEquals(List.of("_raw_spin_unlock_irqrestore", "inlined_frame_1", "inlined_frame_0"), stackFrame.functionName);
        assertNotNull(response.getStackTraceEvents());
        assertEquals(1, (int) response.getStackTraceEvents().get("QjoLteG7HX3VUUXr-J4kHQ"));

        assertNotNull(response.getExecutables());
        assertNotNull("libc.so.6", response.getExecutables().get("QCCDqjSg3bMK1C4YRK6Tiw"));
    }
}
