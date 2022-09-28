import { createSVGIcon, isInputElement } from './dom'

describe('isInputElement', () => {
    test('detect <input> elements as input', () => {
        const element = document.createElement('input')
        expect(isInputElement(element)).toBe(true)
    })

    test('detect <textarea> elements as input', () => {
        const element = document.createElement('textarea')
        expect(isInputElement(element)).toBe(true)
    })

    test('detect contenteditable elements as input', () => {
        const element = document.createElement('div')
        element.contentEditable = ''
        expect(isInputElement(element)).toBe(true)

        element.contentEditable = 'true'
        expect(isInputElement(element)).toBe(true)

        element.contentEditable = 'invalid value'
        expect(isInputElement(element)).toBe(false)
    })

    test('detect content editable elements inherited', () => {
        const parent = document.createElement('div')
        parent.contentEditable = ''

        const element = document.createElement('span')
        parent.append(element)

        expect(isInputElement(element)).toBe(true)
    })
})

describe('createSVGIcon', () => {
    test('create SVG icon', () => {
        expect(createSVGIcon('M 10 10')).toMatchInlineSnapshot(`
            <svg
              aria-hidden="true"
              viewBox="0 0 24 24"
            >
              <path
                d="M 10 10"
              />
            </svg>
        `)
    })
})
