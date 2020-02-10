/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.eql.plan.physical;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.xpack.eql.planner.PlanningException;
import org.elasticsearch.xpack.eql.session.EqlSession;
import org.elasticsearch.xpack.eql.session.Executable;
import org.elasticsearch.xpack.eql.session.Results;


// this is mainly a marker interface to validate a plan before being executed
public interface Unexecutable extends Executable {

    @Override
    default void execute(EqlSession session, ActionListener<Results> listener) {
        throw new PlanningException("Current plan {} is not executable", this);
    }
}
