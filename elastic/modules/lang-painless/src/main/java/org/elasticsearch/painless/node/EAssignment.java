/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.painless.node;


import org.elasticsearch.painless.AnalyzerCaster;
import org.elasticsearch.painless.Location;
import org.elasticsearch.painless.Operation;
import org.elasticsearch.painless.Scope;
import org.elasticsearch.painless.ir.AssignmentNode;
import org.elasticsearch.painless.ir.BinaryMathNode;
import org.elasticsearch.painless.ir.BraceNode;
import org.elasticsearch.painless.ir.BraceSubDefNode;
import org.elasticsearch.painless.ir.ClassNode;
import org.elasticsearch.painless.ir.DotNode;
import org.elasticsearch.painless.ir.DotSubDefNode;
import org.elasticsearch.painless.ir.ExpressionNode;
import org.elasticsearch.painless.lookup.PainlessCast;
import org.elasticsearch.painless.lookup.def;
import org.elasticsearch.painless.symbol.ScriptRoot;

import java.util.Objects;

/**
 * Represents an assignment with the lhs and rhs as child nodes.
 */
public class EAssignment extends AExpression {

    protected final AExpression lhs;
    protected final AExpression rhs;
    protected final boolean pre;
    protected final boolean post;
    protected final Operation operation;

    public EAssignment(Location location, AExpression lhs, AExpression rhs, boolean pre, boolean post, Operation operation) {
        super(location);

        this.lhs = Objects.requireNonNull(lhs);
        this.rhs = rhs;
        this.pre = pre;
        this.post = post;
        this.operation = operation;
    }

    @Override
    Output analyze(ClassNode classNode, ScriptRoot scriptRoot, Scope scope, Input input) {
        Output output = new Output();

        AExpression rhs = this.rhs;
        Operation operation = this.operation;
        boolean cat = false;
        Class<?> promote = null;
        Class<?> shiftDistance = null;
        PainlessCast there = null;
        PainlessCast back = null;

        Output leftOutput;

        Input rightInput = new Input();
        Output rightOutput;

        if (lhs instanceof AStoreable) {
            AStoreable lhs = (AStoreable)this.lhs;
            AStoreable.Input leftInput = new AStoreable.Input();

            leftInput.read = input.read;
            leftInput.write = true;
            leftOutput = lhs.analyze(classNode, scriptRoot, scope, leftInput);
        } else {
            throw new IllegalArgumentException("Left-hand side cannot be assigned a value.");
        }

        if (pre && post) {
            throw createError(new IllegalStateException("Illegal tree structure."));
        } else if (pre || post) {
            if (rhs != null) {
                throw createError(new IllegalStateException("Illegal tree structure."));
            }

            if (operation == Operation.INCR) {
                if (leftOutput.actual == double.class) {
                    rhs = new EConstant(location, 1D);
                } else if (leftOutput.actual == float.class) {
                    rhs = new EConstant(location, 1F);
                } else if (leftOutput.actual == long.class) {
                    rhs = new EConstant(location, 1L);
                } else {
                    rhs = new EConstant(location, 1);
                }

                operation = Operation.ADD;
            } else if (operation == Operation.DECR) {
                if (leftOutput.actual == double.class) {
                    rhs = new EConstant(location, 1D);
                } else if (leftOutput.actual == float.class) {
                    rhs = new EConstant(location, 1F);
                } else if (leftOutput.actual == long.class) {
                    rhs = new EConstant(location, 1L);
                } else {
                    rhs = new EConstant(location, 1);
                }

                operation = Operation.SUB;
            } else {
                throw createError(new IllegalStateException("Illegal tree structure."));
            }
        }

        if (operation != null) {
            rightOutput = rhs.analyze(classNode, scriptRoot, scope, rightInput);
            boolean shift = false;

            if (operation == Operation.MUL) {
                promote = AnalyzerCaster.promoteNumeric(leftOutput.actual, rightOutput.actual, true);
            } else if (operation == Operation.DIV) {
                promote = AnalyzerCaster.promoteNumeric(leftOutput.actual, rightOutput.actual, true);
            } else if (operation == Operation.REM) {
                promote = AnalyzerCaster.promoteNumeric(leftOutput.actual, rightOutput.actual, true);
            } else if (operation == Operation.ADD) {
                promote = AnalyzerCaster.promoteAdd(leftOutput.actual, rightOutput.actual);
            } else if (operation == Operation.SUB) {
                promote = AnalyzerCaster.promoteNumeric(leftOutput.actual, rightOutput.actual, true);
            } else if (operation == Operation.LSH) {
                promote = AnalyzerCaster.promoteNumeric(leftOutput.actual, false);
                shiftDistance = AnalyzerCaster.promoteNumeric(rightOutput.actual, false);
                shift = true;
            } else if (operation == Operation.RSH) {
                promote = AnalyzerCaster.promoteNumeric(leftOutput.actual, false);
                shiftDistance = AnalyzerCaster.promoteNumeric(rightOutput.actual, false);
                shift = true;
            } else if (operation == Operation.USH) {
                promote = AnalyzerCaster.promoteNumeric(leftOutput.actual, false);
                shiftDistance = AnalyzerCaster.promoteNumeric(rightOutput.actual, false);
                shift = true;
            } else if (operation == Operation.BWAND) {
                promote = AnalyzerCaster.promoteXor(leftOutput.actual, rightOutput.actual);
            } else if (operation == Operation.XOR) {
                promote = AnalyzerCaster.promoteXor(leftOutput.actual, rightOutput.actual);
            } else if (operation == Operation.BWOR) {
                promote = AnalyzerCaster.promoteXor(leftOutput.actual, rightOutput.actual);
            } else {
                throw createError(new IllegalStateException("Illegal tree structure."));
            }

            if (promote == null || (shift && shiftDistance == null)) {
                throw createError(new ClassCastException("Cannot apply compound assignment " +
                        "[" + operation.symbol + "=] to types [" + leftOutput.actual + "] and [" + rightOutput.actual + "]."));
            }

            cat = operation == Operation.ADD && promote == String.class;

            if (cat) {
                if (rhs instanceof EBinary && ((EBinary)rhs).operation == Operation.ADD && rightOutput.actual == String.class) {
                    ((BinaryMathNode)rightOutput.expressionNode).setCat(true);
                }
            }

            if (shift) {
                if (promote == def.class) {
                    // shifts are promoted independently, but for the def type, we need object.
                    rightInput.expected = promote;
                } else if (shiftDistance == long.class) {
                    rightInput.expected = int.class;
                    rightInput.explicit = true;
                } else {
                    rightInput.expected = shiftDistance;
                }
            } else {
                rightInput.expected = promote;
            }

            rhs.cast(rightInput, rightOutput);

            there = AnalyzerCaster.getLegalCast(location, leftOutput.actual, promote, false, false);
            back = AnalyzerCaster.getLegalCast(location, promote, leftOutput.actual, true, false);


        } else if (rhs != null) {
            AStoreable lhs = (AStoreable)this.lhs;

            // TODO: move this optimization to a later phase
            // If the lhs node is a def optimized node we update the actual type to remove the need for a cast.
            if (lhs.isDefOptimized()) {
                rightOutput = rhs.analyze(classNode, scriptRoot, scope, rightInput);

                if (rightOutput.actual == void.class) {
                    throw createError(new IllegalArgumentException("Right-hand side cannot be a [void] type for assignment."));
                }

                rightInput.expected = rightOutput.actual;
                leftOutput.actual = rightOutput.actual;
                leftOutput.expressionNode.setExpressionType(rightOutput.actual);

                ExpressionNode expressionNode = leftOutput.expressionNode;

                if (expressionNode instanceof DotNode && ((DotNode)expressionNode).getRightNode() instanceof DotSubDefNode) {
                    ((DotNode)expressionNode).getRightNode().setExpressionType(leftOutput.actual);
                } else if (expressionNode instanceof BraceNode && ((BraceNode)expressionNode).getRightNode() instanceof BraceSubDefNode) {
                    ((BraceNode)expressionNode).getRightNode().setExpressionType(leftOutput.actual);
                }
            // Otherwise, we must adapt the rhs type to the lhs type with a cast.
            } else {
                rightInput.expected = leftOutput.actual;
                rightOutput = rhs.analyze(classNode, scriptRoot, scope, rightInput);
            }

            rhs.cast(rightInput, rightOutput);
        } else {
            throw new IllegalStateException("Illegal tree structure.");
        }

        output.actual = input.read ? leftOutput.actual : void.class;

        AssignmentNode assignmentNode = new AssignmentNode();

        assignmentNode.setLeftNode(leftOutput.expressionNode);
        assignmentNode.setRightNode(rhs.cast(rightOutput));

        assignmentNode.setLocation(location);
        assignmentNode.setExpressionType(output.actual);
        assignmentNode.setCompoundType(promote);
        assignmentNode.setPre(pre);
        assignmentNode.setPost(post);
        assignmentNode.setOperation(operation);
        assignmentNode.setRead(input.read);
        assignmentNode.setCat(cat);
        assignmentNode.setThere(there);
        assignmentNode.setBack(back);

        output.expressionNode = assignmentNode;

        return output;
    }
}
