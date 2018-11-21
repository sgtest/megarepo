import React from 'react'
import * as GQL from '../../../../../shared/src/graphql/schema'

/** Displays the price of a plan. */
export const ProductPlanPrice: React.FunctionComponent<{
    pricePerUserPerYear: GQL.IProductPlan['pricePerUserPerYear']
}> = ({ pricePerUserPerYear }) => (
    <>
        {(pricePerUserPerYear / 100 /* cents in a USD */ / 12) /* months */
            .toLocaleString('en-US', { style: 'currency', currency: 'USD', minimumFractionDigits: 0 })}
        /user/month (paid yearly)
    </>
)
