/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.analysis.analyzer;

import java.util.Collection;
import java.util.stream.Collectors;

import org.elasticsearch.xpack.sql.analysis.AnalysisException;
import org.elasticsearch.xpack.sql.analysis.analyzer.Verifier.Failure;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.util.StringUtils;


public class VerificationException extends AnalysisException {

    private final Collection<Failure> failures;

    protected VerificationException(Collection<Failure> sources) {
        super(null, StringUtils.EMPTY);
        failures = sources;
    }

    @Override
    public String getMessage() {
        return failures.stream()
                .map(f -> {
                    Location l = f.source().location();
                    return "line " + l.getLineNumber() + ":" + l.getColumnNumber() + ": " + f.message();
                })
                .collect(Collectors.joining(StringUtils.NEW_LINE, "Found " + failures.size() + " problem(s)\n", StringUtils.EMPTY));
    }
}
