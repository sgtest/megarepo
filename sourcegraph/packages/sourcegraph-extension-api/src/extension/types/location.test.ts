import { assertToJSON } from './common.test'
import { Location } from './location'
import { Position } from './position'
import { Range } from './range'
import { URI } from './uri'

describe('Location', () => {
    it('toJSON', () => {
        assertToJSON(new Location(URI.file('u.ts'), new Position(3, 4)), {
            uri: URI.parse('file://u.ts').toJSON(),
            range: { start: { line: 3, character: 4 }, end: { line: 3, character: 4 } },
        })
        assertToJSON(new Location(URI.file('u.ts'), new Range(1, 2, 3, 4)), {
            uri: URI.parse('file://u.ts').toJSON(),
            range: { start: { line: 1, character: 2 }, end: { line: 3, character: 4 } },
        })
    })
})
