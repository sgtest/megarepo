import classNames from 'classnames'

import { MenuItem } from '@sourcegraph/wildcard'

import styles from './Recipes.module.scss'

export interface RecipeActionProps {
    title: string
    onClick: () => void
    disabled?: boolean
}

export const RecipeAction = ({ title, onClick, disabled }: RecipeActionProps): JSX.Element => (
    <MenuItem className={classNames(styles.recipeMenuWrapper)} onSelect={onClick} disabled={disabled}>
        {title}
    </MenuItem>
)
