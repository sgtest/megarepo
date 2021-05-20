import * as React from 'react'

import { CtaBanner } from '../../components/CtaBanner'

interface Props {
    className?: string
}

export const SignUpCta: React.FunctionComponent<Props> = ({ className }) => (
    <CtaBanner
        className={className}
        icon={<MagnifyingGlassIllustration />}
        title="Improve your workflow"
        bodyText="Sign up to add your code, monitor searches for changes, and access additional search features."
        linkText="Sign up"
        href="/sign-up"
        googleAnalytics={true}
    />
)

const MagnifyingGlassIllustration = React.memo(() => (
    <svg width="55" height="55" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path
            d="M21.5 24.644L25.517 27l-1.066-4.44L28 19.573l-4.674-.392L21.5 15l-1.826 4.181-4.674.392 3.543 2.987-1.06 4.44 4.017-2.356z"
            fill="#F96216"
        />
        <path
            d="M21.481 6.783A14.697 14.697 0 0136.18 21.481c0 3.64-1.334 6.987-3.528 9.565l.61.61h1.787l11.306 11.306-3.392 3.392-11.306-11.306v-1.786l-.61-.61a14.734 14.734 0 01-9.565 3.527 14.697 14.697 0 110-29.396zm0 4.523A10.133 10.133 0 0011.306 21.48 10.133 10.133 0 0021.48 31.656a10.133 10.133 0 0010.175-10.175 10.133 10.133 0 00-10.175-10.175z"
            fill="#A6B6D9"
        />
    </svg>
))
