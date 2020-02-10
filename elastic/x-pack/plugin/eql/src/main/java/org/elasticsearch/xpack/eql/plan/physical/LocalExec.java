/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.eql.plan.physical;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.xpack.eql.session.EmptyExecutable;
import org.elasticsearch.xpack.eql.session.EqlSession;
import org.elasticsearch.xpack.eql.session.Executable;
import org.elasticsearch.xpack.eql.session.Results;
import org.elasticsearch.xpack.ql.expression.Attribute;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;

import java.util.List;
import java.util.Objects;

public class LocalExec extends LeafExec {

    private final Executable executable;

    public LocalExec(Source source, Executable executable) {
        super(source);
        this.executable = executable;
    }

    @Override
    protected NodeInfo<LocalExec> info() {
        return NodeInfo.create(this, LocalExec::new, executable);
    }

    public Executable executable() {
        return executable;
    }

    @Override
    public List<Attribute> output() {
        return executable.output();
    }

    public boolean isEmpty() {
        return executable instanceof EmptyExecutable;
    }

    @Override
    public void execute(EqlSession session, ActionListener<Results> listener) {
        executable.execute(session, listener);
    }

    @Override
    public int hashCode() {
        return Objects.hash(executable);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        LocalExec other = (LocalExec) obj;
        return Objects.equals(executable, other.executable);
    }
}
