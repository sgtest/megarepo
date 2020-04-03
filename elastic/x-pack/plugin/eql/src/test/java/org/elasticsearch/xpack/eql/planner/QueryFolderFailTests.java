/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.eql.planner;

import org.elasticsearch.xpack.eql.analysis.VerificationException;
import org.elasticsearch.xpack.ql.ParsingException;
import org.elasticsearch.xpack.ql.QlIllegalArgumentException;

public class QueryFolderFailTests extends AbstractQueryFolderTestCase {
    public void testPropertyEquationFilterUnsupported() {
        QlIllegalArgumentException e = expectThrows(QlIllegalArgumentException.class,
                () -> plan("process where (serial_event_id<9 and serial_event_id >= 7) or (opcode == pid)"));
        String msg = e.getMessage();
        assertEquals("Line 1:74: Comparisons against variables are not (currently) supported; offender [pid] in [==]", msg);
    }

    public void testPropertyEquationInClauseFilterUnsupported() {
        VerificationException e = expectThrows(VerificationException.class,
                () -> plan("process where opcode in (1,3) and process_name in (parent_process_name, \"SYSTEM\")"));
        String msg = e.getMessage();
        assertEquals("Found 1 problem\nline 1:35: Comparisons against variables are not (currently) supported; " +
            "offender [parent_process_name] in [process_name in (parent_process_name, \"SYSTEM\")]", msg);
    }

    public void testLengthFunctionWithInexact() {
        VerificationException e = expectThrows(VerificationException.class,
                () -> plan("process where length(plain_text) > 0"));
        String msg = e.getMessage();
        assertEquals("Found 1 problem\nline 1:15: [length(plain_text)] cannot operate on field of data type [text]: No keyword/multi-field "
                + "defined exact matches for [plain_text]; define one or use MATCH/QUERY instead", msg);
    }

    public void testEndsWithFunctionWithInexact() {
        VerificationException e = expectThrows(VerificationException.class,
                () -> plan("process where endsWith(plain_text, \"foo\") == true"));
        String msg = e.getMessage();
        assertEquals("Found 1 problem\nline 1:15: [endsWith(plain_text, \"foo\")] cannot operate on first argument field of data type "
                + "[text]: No keyword/multi-field defined exact matches for [plain_text]; define one or use MATCH/QUERY instead", msg);
    }

    public void testStartsWithFunctionWithInexact() {
        VerificationException e = expectThrows(VerificationException.class,
                () -> plan("process where startsWith(plain_text, \"foo\") == true"));
        String msg = e.getMessage();
        assertEquals("Found 1 problem\nline 1:15: [startsWith(plain_text, \"foo\")] cannot operate on first argument field of data type "
                + "[text]: No keyword/multi-field defined exact matches for [plain_text]; define one or use MATCH/QUERY instead", msg);
    }

    public void testWildcardNotEnoughArguments() {
        ParsingException e = expectThrows(ParsingException.class,
            () -> plan("process where wildcard(process_name)"));
        String msg = e.getMessage();
        assertEquals("line 1:16: error building [wildcard]: expects at least two arguments", msg);
    }

    public void testWildcardAgainstVariable() {
        VerificationException e = expectThrows(VerificationException.class,
            () -> plan("process where wildcard(process_name, parent_process_name)"));
        String msg = e.getMessage();
        assertEquals("Found 1 problem\nline 1:15: second argument of [wildcard(process_name, parent_process_name)] " +
            "must be a constant, received [parent_process_name]", msg);
    }

    public void testWildcardWithNumericPattern() {
        VerificationException e = expectThrows(VerificationException.class,
            () -> plan("process where wildcard(process_name, 1)"));
        String msg = e.getMessage();
        assertEquals("Found 1 problem\n" +
            "line 1:15: second argument of [wildcard(process_name, 1)] must be [string], found value [1] type [integer]", msg);
    }

    public void testWildcardWithNumericField() {
        VerificationException e = expectThrows(VerificationException.class,
            () -> plan("process where wildcard(pid, '*.exe')"));
        String msg = e.getMessage();
        assertEquals("Found 1 problem\n" +
            "line 1:15: first argument of [wildcard(pid, '*.exe')] must be [string], found value [pid] type [long]", msg);
    }
}
