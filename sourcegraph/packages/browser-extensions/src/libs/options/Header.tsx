import * as React from 'react'

export interface OptionsHeaderProps {
    className?: string
    version: string
    assetsDir?: string
}

export const OptionsHeader: React.SFC<OptionsHeaderProps> = ({ className, version, assetsDir }: OptionsHeaderProps) => (
    <div className={`options-header ${className || ''}`}>
        <img src={`${assetsDir || ''}/img/sourcegraph-logo.svg`} className="options-header__logo" />
        <div className="options-header__right">
            <span>v{version}</span>
        </div>
    </div>
)
