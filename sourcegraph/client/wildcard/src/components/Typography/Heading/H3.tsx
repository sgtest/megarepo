import React from 'react'

import { ForwardReferenceComponent } from '../../../types'

import { Heading, HeadingProps } from './Heading'

type H3Props = HeadingProps

// eslint-disable-next-line id-length
export const H3 = React.forwardRef(function H3({ children, as = 'h3', ...props }, reference) {
    return (
        <Heading as={as} styleAs="h3" {...props} ref={reference}>
            {children}
        </Heading>
    )
}) as ForwardReferenceComponent<'h3', H3Props>
