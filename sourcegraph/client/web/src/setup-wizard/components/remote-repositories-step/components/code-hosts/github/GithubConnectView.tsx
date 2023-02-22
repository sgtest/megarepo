import { FC, ReactNode, ReactElement, useCallback, useState, useMemo, ChangeEvent } from 'react'

import classNames from 'classnames'
import { parse as parseJSONC } from 'jsonc-parser'
import { noop } from 'lodash'

import { modify } from '@sourcegraph/common'
import {
    Tabs,
    Tab,
    TabList,
    TabPanel,
    TabPanels,
    Input,
    Checkbox,
    useLocalStorage,
    useField,
    useForm,
    FormInstance,
    getDefaultInputProps,
    useFieldAPI,
    useControlledField,
    ErrorAlert,
    FORM_ERROR,
} from '@sourcegraph/wildcard'

import { codeHostExternalServices } from '../../../../../../components/externalServices/externalServices'
import { AddExternalServiceInput, ExternalServiceKind } from '../../../../../../graphql-operations'
import { CodeHostJSONFormContent, RadioGroupSection, CodeHostConnectFormFields, CodeHostJSONFormState } from '../common'

import { GithubOrganizationsPicker, GithubRepositoriesPicker } from './GithubEntityPickers'

import styles from './GithubConnectView.module.scss'

const DEFAULT_FORM_VALUES: CodeHostConnectFormFields = {
    displayName: codeHostExternalServices.github.defaultDisplayName,
    config: `
{
    "url": "https://github.com",
    "token": ""
}
`.trim(),
}

interface GithubConnectViewProps {
    initialValues?: CodeHostConnectFormFields

    /**
     * Render props that is connected to form state, usually is used to render
     * form actions UI, like save, cancel, clear fields. Action layout is the same
     * for all variations of this form (create, edit UI) but content is different
     */
    children: (state: CodeHostJSONFormState) => ReactNode
    onSubmit: (input: AddExternalServiceInput) => Promise<any>
}

/**
 * GitHub's creation UI panel, it renders GitHub connection form UI and also handles
 * form values logic, like saving work-in-progress form values in local
 * storage
 */
export const GithubConnectView: FC<GithubConnectViewProps> = props => {
    const { initialValues, children, onSubmit } = props
    const [localValues, setInitialValues] = useLocalStorage('github-connection-form', DEFAULT_FORM_VALUES)

    const handleSubmit = useCallback(
        async (values: CodeHostConnectFormFields): Promise<void> => {
            // Perform public API code host connection create action
            await onSubmit({
                kind: ExternalServiceKind.GITHUB,
                displayName: values.displayName,
                config: values.config,
            })

            // Reset initial values after successful connect action
            setInitialValues(DEFAULT_FORM_VALUES)
        },
        [setInitialValues, onSubmit]
    )

    return (
        <GithubConnectForm
            initialValues={initialValues ?? localValues}
            onChange={initialValues ? noop : setInitialValues}
            onSubmit={handleSubmit}
        >
            {children}
        </GithubConnectForm>
    )
}

enum GithubConnectFormTab {
    Form,
    JSONC,
}

interface GithubConnectFormProps {
    initialValues: CodeHostConnectFormFields
    children: (state: CodeHostJSONFormState) => ReactNode
    onChange: (values: CodeHostConnectFormFields) => void
    onSubmit: (values: CodeHostConnectFormFields) => void
}

/**
 * It renders custom GitHub connect form that provides form UI and plain JSONC
 * configuration UI.
 */
export const GithubConnectForm: FC<GithubConnectFormProps> = props => {
    const { initialValues, children, onChange, onSubmit } = props

    const [activeTab, setActiveTab] = useState(GithubConnectFormTab.Form)
    const form = useForm<CodeHostConnectFormFields>({
        initialValues,
        onSubmit,
        onChange: event => onChange(event.values),
    })

    const displayName = useField({
        formApi: form.formAPI,
        name: 'displayName',
        required: true,
    })

    const configuration = useField({
        formApi: form.formAPI,
        name: 'config',
    })

    return (
        <Tabs
            as="form"
            index={activeTab}
            lazy={true}
            size="medium"
            behavior="memoize"
            className={styles.form}
            onChange={setActiveTab}
            ref={form.ref}
            onSubmit={form.handleSubmit}
        >
            <TabList wrapperClassName={styles.tabList}>
                <Tab index={GithubConnectFormTab.Form} className={styles.tab}>
                    Settings
                </Tab>
                <Tab index={GithubConnectFormTab.JSONC} className={styles.tab}>
                    JSONC editor
                </Tab>
            </TabList>
            <TabPanels className={styles.tabPanels}>
                <TabPanel as="fieldset" tabIndex={-1} className={styles.formView}>
                    <GithubFormView
                        form={form}
                        displayNameField={displayName}
                        configurationField={configuration}
                        isTabActive={activeTab === GithubConnectFormTab.Form}
                    />
                </TabPanel>
                <TabPanel as="fieldset" tabIndex={-1} className={styles.formView}>
                    <CodeHostJSONFormContent
                        displayNameField={displayName}
                        configurationField={configuration}
                        externalServiceOptions={codeHostExternalServices.github}
                    />
                </TabPanel>
                <>
                    {form.formAPI.submitErrors && (
                        <ErrorAlert className="w-100 mt-4" error={form.formAPI.submitErrors[FORM_ERROR]} />
                    )}
                </>
            </TabPanels>

            {children(form.formAPI)}
        </Tabs>
    )
}

interface GithubFormViewProps {
    form: FormInstance<CodeHostConnectFormFields>
    displayNameField: useFieldAPI<string>
    configurationField: useFieldAPI<string>
    isTabActive: boolean
}

function GithubFormView(props: GithubFormViewProps): ReactElement {
    const { form, displayNameField, configurationField } = props

    const accessTokenField = useControlledField({
        value: getAccessTokenValue(configurationField.input.value),
        name: 'accessToken',
        submitted: form.formAPI.submitted,
        formTouched: form.formAPI.touched,
        validators: { sync: syncAccessTokenValidator, async: asyncAccessTokenValidator },
        onChange: value => configurationField.input.onChange(modify(configurationField.input.value, ['token'], value)),
    })

    const { isAffiliatedRepositories, isOrgsRepositories, isSetRepositories, organizations, repositories } = useMemo(
        () => getRepositoriesSettings(configurationField.input.value),
        [configurationField.input.value]
    )

    const handleAffiliatedModeChange = (event: ChangeEvent<HTMLInputElement>): void => {
        const parsedConfiguration = parseJSONC(configurationField.input.value) as Record<string, any>
        const reposQuery: string[] =
            typeof parsedConfiguration === 'object' ? [...(parsedConfiguration.reposQuery ?? [])] : []

        const nextReposQuery = event.target.checked
            ? [...reposQuery, 'affiliated']
            : reposQuery.filter(token => token !== 'affiliated')

        configurationField.input.onChange(modify(configurationField.input.value, ['repositoryQuery'], nextReposQuery))
    }

    const handleOrganizationsModeChange = (event: ChangeEvent<HTMLInputElement>): void => {
        const nextConfiguration = event.target.checked
            ? modify(configurationField.input.value, ['orgs'], [])
            : modify(configurationField.input.value, ['orgs'], undefined)

        configurationField.input.onChange(nextConfiguration)
    }

    const handleRepositoriesModeChange = (event: ChangeEvent<HTMLInputElement>): void => {
        const nextConfiguration = event.target.checked
            ? modify(configurationField.input.value, ['repos'], [])
            : modify(configurationField.input.value, ['repos'], undefined)

        configurationField.input.onChange(nextConfiguration)
    }

    const handleOrganizationsChange = (organizations: string[]): void => {
        configurationField.input.onChange(modify(configurationField.input.value, ['orgs'], organizations))
    }

    const handleRepositoriesChange = (repositories: string[]): void => {
        configurationField.input.onChange(modify(configurationField.input.value, ['repos'], repositories))
    }

    // Fragment to avoid nesting since it's rendered within TabPanel fieldset
    return (
        <>
            <Input label="Display name" placeholder="Github (Personal)" {...getDefaultInputProps(displayNameField)} />

            <Input
                label="Access token"
                placeholder="Input your access token"
                message="Create a new access token on GitHub.com with repo or public_repo scope."
                {...getDefaultInputProps(accessTokenField)}
            />

            <section
                className={classNames(styles.repositoriesFields, {
                    [styles.repositoriesFieldsDisabled]: accessTokenField.meta.validState !== 'VALID',
                })}
            >
                <Checkbox
                    id="all-repos"
                    name="repositories"
                    label="Add all my repositories"
                    message="Will add all repositories affiliated with the token"
                    checked={isAffiliatedRepositories}
                    onChange={handleAffiliatedModeChange}
                />

                <RadioGroupSection
                    name="orgs"
                    value="orgs-repos"
                    checked={isOrgsRepositories}
                    labelId="orgs-repos"
                    label="Add all repositories from selected organizations or users"
                    onChange={handleOrganizationsModeChange}
                >
                    <GithubOrganizationsPicker organizations={organizations} onChange={handleOrganizationsChange} />
                </RadioGroupSection>

                <RadioGroupSection
                    name="repositories"
                    value="repositories"
                    checked={isSetRepositories}
                    labelId="repos"
                    label="Add selected repositories"
                    onChange={handleRepositoriesModeChange}
                >
                    <GithubRepositoriesPicker repositories={repositories} onChange={handleRepositoriesChange} />
                </RadioGroupSection>
            </section>
        </>
    )
}

function syncAccessTokenValidator(value: string | undefined): string | undefined {
    if (!value || value.length === 0) {
        return 'Access token is a required field'
    }

    return
}

async function asyncAccessTokenValidator(value: string | undefined): Promise<string | undefined> {
    if (!value) {
        return
    }

    await new Promise(res => setTimeout(res, 1000))

    return
}

function getAccessTokenValue(configuration: string): string {
    const parsedConfiguration = parseJSONC(configuration) as Record<string, any>

    if (typeof parsedConfiguration === 'object') {
        return parsedConfiguration.token ?? ''
    }

    return ''
}

interface GithubFormConfiguration {
    isAffiliatedRepositories: boolean
    isOrgsRepositories: boolean
    isSetRepositories: boolean
    repositories: string[]
    organizations: string[]
}

function getRepositoriesSettings(configuration: string): GithubFormConfiguration {
    const parsedConfiguration = parseJSONC(configuration) as Record<string, any>

    if (typeof parsedConfiguration === 'object') {
        const repositoryQuery: string[] = parsedConfiguration.repositoryQuery ?? []

        return {
            isAffiliatedRepositories: repositoryQuery.includes('affiliated'),
            isOrgsRepositories: Array.isArray(parsedConfiguration.orgs),
            organizations: parsedConfiguration.orgs ?? [],
            isSetRepositories: Array.isArray(parsedConfiguration.repos),
            repositories: parsedConfiguration.repos ?? [],
        }
    }

    return {
        isAffiliatedRepositories: false,
        isOrgsRepositories: false,
        organizations: [],
        isSetRepositories: false,
        repositories: [],
    }
}
