import { assertToJSON } from './common.test'
import { Selection } from './selection'

describe('Selection', () => {
    it('toJSON', () => {
        assertToJSON(new Selection(3, 4, 2, 1), {
            start: { line: 2, character: 1 },
            end: { line: 3, character: 4 },
            anchor: { line: 3, character: 4 },
            active: { line: 2, character: 1 },
        })
    })
})
