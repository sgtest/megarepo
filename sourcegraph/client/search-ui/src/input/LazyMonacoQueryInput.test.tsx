import { render } from '@testing-library/react'
import { noop } from 'lodash'

import { PlainQueryInput } from './LazyMonacoQueryInput'

describe('PlainQueryInput', () => {
    test('empty', () =>
        expect(
            render(
                <PlainQueryInput
                    queryState={{
                        query: '',
                    }}
                    onChange={noop}
                />
            ).asFragment()
        ).toMatchSnapshot())

    test('with query', () =>
        expect(
            render(
                <PlainQueryInput
                    queryState={{
                        query: 'repo:jsonrpc2 file:async.go asyncHandler',
                    }}
                    onChange={noop}
                />
            ).asFragment()
        ).toMatchSnapshot())
})
