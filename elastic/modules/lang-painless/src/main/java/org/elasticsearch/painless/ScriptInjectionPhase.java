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

package org.elasticsearch.painless;

import org.elasticsearch.painless.ir.BlockNode;
import org.elasticsearch.painless.ir.CallNode;
import org.elasticsearch.painless.ir.CallSubNode;
import org.elasticsearch.painless.ir.CatchNode;
import org.elasticsearch.painless.ir.ClassNode;
import org.elasticsearch.painless.ir.DeclarationNode;
import org.elasticsearch.painless.ir.FunctionNode;
import org.elasticsearch.painless.ir.StatementNode;
import org.elasticsearch.painless.ir.StaticNode;
import org.elasticsearch.painless.ir.ThrowNode;
import org.elasticsearch.painless.ir.TryNode;
import org.elasticsearch.painless.ir.MemberCallNode;
import org.elasticsearch.painless.ir.MemberFieldNode;
import org.elasticsearch.painless.ir.VariableNode;
import org.elasticsearch.painless.lookup.PainlessLookup;
import org.elasticsearch.painless.lookup.PainlessMethod;
import org.elasticsearch.painless.symbol.FunctionTable.LocalFunction;
import org.elasticsearch.painless.symbol.ScriptRoot;
import org.elasticsearch.script.ScriptException;
import org.objectweb.asm.commons.Method;

import java.util.Arrays;
import java.util.Collections;
import java.util.Map;

/**
 * This injects additional ir nodes required for
 * the "execute" method. This includes injection of ir nodes
 * to convert get methods into local variables for those
 * that are used and adds additional sandboxing by wrapping
 * the main "execute" block with several exceptions.
 */
public class ScriptInjectionPhase {

    public static void phase(ScriptRoot scriptRoot, ClassNode classNode) {
        FunctionNode executeFunctionNode = null;

        // look up the execute method for decoration
        for (FunctionNode functionNode : classNode.getFunctionsNodes()) {
            if ("execute".equals(functionNode.getName())) {
                executeFunctionNode = functionNode;
                break;
            }
        }

        BlockNode blockNode = executeFunctionNode.getBlockNode();

        // convert gets methods to a new set of inserted ir nodes as necessary -
        // requires the gets method name be modified from "getExample" to "example"
        // if a get method variable isn't used it's declaration node is removed from
        // the ir tree permanently so there is no frivolous variable slotting
        int statementIndex = 0;

        while (statementIndex < blockNode.getStatementsNodes().size()) {
            StatementNode statementNode = blockNode.getStatementsNodes().get(statementIndex);

            if (statementNode instanceof DeclarationNode) {
                DeclarationNode declarationNode = (DeclarationNode)statementNode;
                boolean isRemoved = false;

                for (int getIndex = 0; getIndex < scriptRoot.getScriptClassInfo().getGetMethods().size(); ++getIndex) {
                    Class<?> returnType = scriptRoot.getScriptClassInfo().getGetReturns().get(getIndex);
                    Method getMethod = scriptRoot.getScriptClassInfo().getGetMethods().get(getIndex);
                    String name = getMethod.getName().substring(3);
                    name = Character.toLowerCase(name.charAt(0)) + name.substring(1);

                    if (name.equals(declarationNode.getName())) {
                        if (scriptRoot.getUsedVariables().contains(name)) {
                                MemberCallNode memberCallNode = new MemberCallNode();
                                memberCallNode.setLocation(declarationNode.getLocation());
                                memberCallNode.setExpressionType(declarationNode.getDeclarationType());
                                memberCallNode.setLocalFunction(new LocalFunction(
                                        getMethod.getName(), returnType, Collections.emptyList(), true, false));
                                declarationNode.setExpressionNode(memberCallNode);
                        } else {
                            blockNode.getStatementsNodes().remove(statementIndex);
                            isRemoved = true;
                        }

                        break;
                    }
                }

                if (isRemoved == false) {
                    ++statementIndex;
                }
            } else {
                ++statementIndex;
            }
        }

        // decorate the execute method with nodes to wrap the user statements with
        // the sandboxed errors as follows:
        // } catch (PainlessExplainError e) {
        //     throw this.convertToScriptException(e, e.getHeaders($DEFINITION))
        // }
        // and
        // } catch (PainlessError | BootstrapMethodError | OutOfMemoryError | StackOverflowError | Exception e) {
        //     throw this.convertToScriptException(e, e.getHeaders())
        // }
        try {
            Location internalLocation = new Location("<internal>", 0);

            TryNode tryNode = new TryNode();
            tryNode.setLocation(internalLocation);
            tryNode.setBlockNode(blockNode);

            CatchNode catchNode = new CatchNode();
            catchNode.setLocation(internalLocation);

            tryNode.addCatchNode(catchNode);

            DeclarationNode declarationNode = new DeclarationNode();
            declarationNode.setLocation(internalLocation);
            declarationNode.setDeclarationType(PainlessExplainError.class);
            declarationNode.setName("#painlessExplainError");
            declarationNode.setRequiresDefault(false);

            catchNode.setDeclarationNode(declarationNode);

            BlockNode catchBlockNode = new BlockNode();
            catchBlockNode.setLocation(internalLocation);
            catchBlockNode.setAllEscape(true);
            catchBlockNode.setStatementCount(1);

            catchNode.setBlockNode(catchBlockNode);

            ThrowNode throwNode = new ThrowNode();
            throwNode.setLocation(internalLocation);

            catchBlockNode.addStatementNode(throwNode);

            MemberCallNode memberCallNode = new MemberCallNode();
            memberCallNode.setLocation(internalLocation);
            memberCallNode.setExpressionType(ScriptException.class);
            memberCallNode.setLocalFunction(new LocalFunction(
                            "convertToScriptException",
                            ScriptException.class,
                            Arrays.asList(Throwable.class, Map.class),
                            true,
                            false
                    )
            );

            throwNode.setExpressionNode(memberCallNode);

            VariableNode variableNode = new VariableNode();
            variableNode.setLocation(internalLocation);
            variableNode.setExpressionType(ScriptException.class);
            variableNode.setName("#painlessExplainError");

            memberCallNode.addArgumentNode(variableNode);

            CallNode callNode = new CallNode();
            callNode.setLocation(internalLocation);
            callNode.setExpressionType(Map.class);

            memberCallNode.addArgumentNode(callNode);

            variableNode = new VariableNode();
            variableNode.setLocation(internalLocation);
            variableNode.setExpressionType(PainlessExplainError.class);
            variableNode.setName("#painlessExplainError");

            callNode.setLeftNode(variableNode);

            CallSubNode callSubNode = new CallSubNode();
            callSubNode.setLocation(internalLocation);
            callSubNode.setExpressionType(Map.class);
            callSubNode.setBox(PainlessExplainError.class);
            callSubNode.setMethod(new PainlessMethod(
                    PainlessExplainError.class.getMethod(
                            "getHeaders",
                            PainlessLookup.class),
                    PainlessExplainError.class,
                    null,
                    Collections.emptyList(),
                    null,
                    null,
                    null
            ));

            callNode.setRightNode(callSubNode);

            MemberFieldNode memberFieldNode = new MemberFieldNode();
            memberFieldNode.setLocation(internalLocation);
            memberFieldNode.setExpressionType(PainlessLookup.class);
            memberFieldNode.setName("$DEFINITION");
            memberFieldNode.setStatic(true);

            callSubNode.addArgumentNode(memberFieldNode);

            for (Class<?> throwable : new Class<?>[] {
                    PainlessError.class, BootstrapMethodError.class, OutOfMemoryError.class, StackOverflowError.class, Exception.class}) {

                String name = throwable.getSimpleName();
                name = "#" + Character.toLowerCase(name.charAt(0)) + name.substring(1);

                catchNode = new CatchNode();
                catchNode.setLocation(internalLocation);

                tryNode.addCatchNode(catchNode);

                declarationNode = new DeclarationNode();
                declarationNode.setLocation(internalLocation);
                declarationNode.setDeclarationType(throwable);
                declarationNode.setName(name);
                declarationNode.setRequiresDefault(false);

                catchNode.setDeclarationNode(declarationNode);

                catchBlockNode = new BlockNode();
                catchBlockNode.setLocation(internalLocation);
                catchBlockNode.setAllEscape(true);
                catchBlockNode.setStatementCount(1);

                catchNode.setBlockNode(catchBlockNode);

                throwNode = new ThrowNode();
                throwNode.setLocation(internalLocation);

                catchBlockNode.addStatementNode(throwNode);

                memberCallNode = new MemberCallNode();
                memberCallNode.setLocation(internalLocation);
                memberCallNode.setExpressionType(ScriptException.class);
                memberCallNode.setLocalFunction(new LocalFunction(
                                "convertToScriptException",
                                ScriptException.class,
                                Arrays.asList(Throwable.class, Map.class),
                                true,
                                false
                        )
                );

                throwNode.setExpressionNode(memberCallNode);

                variableNode = new VariableNode();
                variableNode.setLocation(internalLocation);
                variableNode.setExpressionType(ScriptException.class);
                variableNode.setName(name);

                memberCallNode.addArgumentNode(variableNode);

                callNode = new CallNode();
                callNode.setLocation(internalLocation);
                callNode.setExpressionType(Map.class);

                memberCallNode.addArgumentNode(callNode);

                StaticNode staticNode = new StaticNode();
                staticNode.setLocation(internalLocation);
                staticNode.setExpressionType(Collections.class);

                callNode.setLeftNode(staticNode);

                callSubNode = new CallSubNode();
                callSubNode.setLocation(internalLocation);
                callSubNode.setExpressionType(Map.class);
                callSubNode.setBox(Collections.class);
                callSubNode.setMethod(new PainlessMethod(
                        Collections.class.getMethod("emptyMap"),
                        Collections.class,
                        null,
                        Collections.emptyList(),
                        null,
                        null,
                        null
                ));

                callNode.setRightNode(callSubNode);
            }

            blockNode = new BlockNode();
            blockNode.setLocation(blockNode.getLocation());
            blockNode.setAllEscape(blockNode.doAllEscape());
            blockNode.setStatementCount(blockNode.getStatementCount());
            blockNode.addStatementNode(tryNode);

            executeFunctionNode.setBlockNode(blockNode);
        } catch (Exception exception) {
            throw new RuntimeException(exception);
        }
    }

    private ScriptInjectionPhase() {
        // do nothing
    }
}
