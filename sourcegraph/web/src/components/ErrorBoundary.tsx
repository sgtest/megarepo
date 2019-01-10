import ErrorIcon from 'mdi-react/ErrorIcon'
import React from 'react'
import { asError } from '../../../shared/src/util/errors'
import { HeroPage } from './HeroPage'

interface State {
    error?: Error
}

/**
 * A [React error boundary](https://reactjs.org/docs/error-boundaries.html) that catches errors from
 * its children. If an error occurs, it displays a nice error page instead of a blank page.
 *
 * Components should handle their own errors (and must not rely on this error boundary). This error
 * boundary is a last resort in case of an unexpected error.
 */
export class ErrorBoundary extends React.PureComponent<{}, State> {
    public state: State = {}

    public static getDerivedStateFromError(error: any): Pick<State, 'error'> {
        return { error: asError(error) }
    }

    public render(): React.ReactNode | null {
        if (this.state.error !== undefined) {
            return (
                <HeroPage
                    icon={ErrorIcon}
                    title="Error"
                    subtitle={
                        <div className="container">
                            <p>
                                Sourcegraph encountered an unexpected error. If reloading the page doesn't fix it,
                                contact your site admin or Sourcegraph support.
                            </p>
                            <p>
                                <code>{this.state.error.message}</code>
                            </p>
                        </div>
                    }
                />
            )
        }

        return this.props.children
    }
}
