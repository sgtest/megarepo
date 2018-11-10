import assert from 'assert'
import { evaluate, evaluateTemplate } from './evaluator'

const FIXTURE_CONTEXT = new Map<string, any>(
    Object.entries({
        a: 1,
        b: 1,
        c: 2,
        x: 'y',
    })
)

describe('evaluate', () => {
    // tslint:disable:no-invalid-template-strings
    const TESTS = {
        a: 1,
        'a + b': 2,
        'a == b': true,
        'a != b': false,
        'a + b == c': true,
        x: 'y',
        '!a': false,
        '!!a': true,
        'a && c': 2,
        'a || b': 1,
        '(a + b) * 2': 4,
        'x == "y"': true,
        '`a`': 'a',
        '`${x}`': 'y',
        '`a${x}b`': 'ayb',
        '`_${x}_${a}_${a+b}`': '_y_1_2',
        '`_${`-${x}-`}_`': '_-y-_',
        'a || isnotdefined': 1, // short-circuit (if not, the use of an undefined ident would cause an error)
    }
    // tslint:enable:no-invalid-template-strings
    for (const [expr, want] of Object.entries(TESTS)) {
        it(expr, () => {
            const value = evaluate(expr, FIXTURE_CONTEXT)
            assert.strictEqual(value, want)
        })
    }
})

describe('evaluateTemplate', () => {
    // tslint:disable:no-invalid-template-strings
    const TESTS = {
        a: 'a',
        '${x}': 'y',
        'a${x}b': 'ayb',
        '_${x}_${a}_${a+b}': '_y_1_2',
        '_${`-${x}-`}_': '_-y-_',
    }
    // tslint:enable:no-invalid-template-strings
    for (const [template, want] of Object.entries(TESTS)) {
        it(template, () => {
            const value = evaluateTemplate(template, FIXTURE_CONTEXT)
            assert.strictEqual(value, want)
        })
    }
})
