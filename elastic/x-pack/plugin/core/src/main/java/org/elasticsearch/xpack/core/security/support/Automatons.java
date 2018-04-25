/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.support;

import org.apache.lucene.util.automaton.Automata;
import org.apache.lucene.util.automaton.Automaton;
import org.apache.lucene.util.automaton.CharacterRunAutomaton;
import org.apache.lucene.util.automaton.RegExp;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.List;
import java.util.function.Predicate;

import static org.apache.lucene.util.automaton.MinimizationOperations.minimize;
import static org.apache.lucene.util.automaton.Operations.DEFAULT_MAX_DETERMINIZED_STATES;
import static org.apache.lucene.util.automaton.Operations.concatenate;
import static org.apache.lucene.util.automaton.Operations.minus;
import static org.apache.lucene.util.automaton.Operations.union;
import static org.elasticsearch.common.Strings.collectionToDelimitedString;

public final class Automatons {

    public static final Automaton EMPTY = Automata.makeEmpty();
    public static final Automaton MATCH_ALL = Automata.makeAnyString();

    static final char WILDCARD_STRING = '*';     // String equality with support for wildcards
    static final char WILDCARD_CHAR = '?';       // Char equality with support for wildcards
    static final char WILDCARD_ESCAPE = '\\';    // Escape character

    private Automatons() {
    }

    /**
     * Builds and returns an automaton that will represent the union of all the given patterns.
     */
    public static Automaton patterns(String... patterns) {
        return patterns(Arrays.asList(patterns));
    }

    /**
     * Builds and returns an automaton that will represent the union of all the given patterns.
     */
    public static Automaton patterns(Collection<String> patterns) {
        if (patterns.isEmpty()) {
            return EMPTY;
        }
        Automaton automaton = null;
        for (String pattern : patterns) {
            final Automaton patternAutomaton = minimize(pattern(pattern), DEFAULT_MAX_DETERMINIZED_STATES);
            automaton = automaton == null ? patternAutomaton : unionAndMinimize(Arrays.asList(automaton, patternAutomaton));
        }
        // the automaton is always minimized and deterministic
        return automaton;
    }

    /**
     * Builds and returns an automaton that represents the given pattern.
     */
    static Automaton pattern(String pattern) {
        if (pattern.startsWith("/")) { // it's a lucene regexp
            if (pattern.length() == 1 || !pattern.endsWith("/")) {
                throw new IllegalArgumentException("invalid pattern [" + pattern + "]. patterns starting with '/' " +
                        "indicate regular expression pattern and therefore must also end with '/'." +
                        " other patterns (those that do not start with '/') will be treated as simple wildcard patterns");
            }
            String regex = pattern.substring(1, pattern.length() - 1);
            return new RegExp(regex).toAutomaton();
        } else if (pattern.equals("*")) {
            return MATCH_ALL;
        } else {
            return wildcard(pattern);
        }
    }

    /**
     * Builds and returns an automaton that represents the given pattern.
     */
    @SuppressWarnings("fallthrough") // explicit fallthrough at end of switch
    static Automaton wildcard(String text) {
        List<Automaton> automata = new ArrayList<>();
        for (int i = 0; i < text.length();) {
            final char c = text.charAt(i);
            int length = 1;
            switch(c) {
                case WILDCARD_STRING:
                    automata.add(Automata.makeAnyString());
                    break;
                case WILDCARD_CHAR:
                    automata.add(Automata.makeAnyChar());
                    break;
                case WILDCARD_ESCAPE:
                    // add the next codepoint instead, if it exists
                    if (i + length < text.length()) {
                        final char nextChar = text.charAt(i + length);
                        length += 1;
                        automata.add(Automata.makeChar(nextChar));
                        break;
                    } // else fallthru, lenient parsing with a trailing \
                default:
                    automata.add(Automata.makeChar(c));
            }
            i += length;
        }
        return concatenate(automata);
    }

    public static Automaton unionAndMinimize(Collection<Automaton> automata) {
        Automaton res = union(automata);
        return minimize(res, DEFAULT_MAX_DETERMINIZED_STATES);
    }

    public static Automaton minusAndMinimize(Automaton a1, Automaton a2) {
        Automaton res = minus(a1, a2, DEFAULT_MAX_DETERMINIZED_STATES);
        return minimize(res, DEFAULT_MAX_DETERMINIZED_STATES);
    }

    public static Predicate<String> predicate(String... patterns) {
        return predicate(Arrays.asList(patterns));
    }

    public static Predicate<String> predicate(Collection<String> patterns) {
        return predicate(patterns(patterns), collectionToDelimitedString(patterns, "|"));
    }

    public static Predicate<String> predicate(Automaton automaton) {
        return predicate(automaton, "Predicate for " + automaton);
    }

    private static Predicate<String> predicate(Automaton automaton, final String toString) {
        CharacterRunAutomaton runAutomaton = new CharacterRunAutomaton(automaton, DEFAULT_MAX_DETERMINIZED_STATES);
        return new Predicate<String>() {
            @Override
            public boolean test(String s) {
                return runAutomaton.run(s);
            }

            @Override
            public String toString() {
                return toString;
            }
        };
    }
}
