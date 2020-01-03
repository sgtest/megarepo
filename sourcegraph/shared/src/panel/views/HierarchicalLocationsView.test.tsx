// react-visibility-sensor, used in CodeExcerpt depends on ReactDOM.findDOMNode,
// which is not supported when using react-test-renderer + jest.
// This mock makes it so that <VisibilitySensor /> simply becomes a <div> in the rendered output.
jest.mock('react-visibility-sensor', () => 'VisibilitySensor')

import { Location } from '@sourcegraph/extension-api-types'
import H from 'history'
import { noop } from 'lodash'
import React from 'react'
import renderer from 'react-test-renderer'
import { concat, NEVER, Observable, of } from 'rxjs'
import * as sinon from 'sinon'
import { createContextService } from '../../api/client/context/contextService'
import { parseTemplate } from '../../api/client/context/expr/evaluator'
import { ContributionsEntry, ContributionUnsubscribable } from '../../api/client/services/contribution'
import { Controller } from '../../extensions/controller'
import { SettingsCascadeOrError } from '../../settings/settings'
import { HierarchicalLocationsView, HierarchicalLocationsViewProps } from './HierarchicalLocationsView'

jest.mock('mdi-react/SourceRepositoryIcon', () => 'SourceRepositoryIcon')

describe('<HierarchicalLocationsView />', () => {
    const getProps = () => {
        const services = {
            context: createContextService({ clientApplication: 'other' }),
            contribution: {
                registerContributions: sinon.spy(
                    (entry: ContributionsEntry): ContributionUnsubscribable => ({ entry, unsubscribe: noop })
                ),
            },
        }
        const extensionsController: Pick<Controller, 'services'> = {
            services: services as any,
        }
        const settingsCascade: SettingsCascadeOrError = {
            subjects: null,
            final: null,
        }
        const location: H.Location = {
            hash: '#L36:18&tab=references',
            pathname: '/github.com/sourcegraph/sourcegraph/-/blob/browser/src/libs/phabricator/index.tsx',
            search: '',
            state: {},
        }

        const props: HierarchicalLocationsViewProps = {
            extensionsController,
            settingsCascade,
            location,
            locations: NEVER,
            defaultGroup: 'git://github.com/foo/bar',
            isLightTheme: true,
            fetchHighlightedFileLines: sinon.spy(),
        }
        return { services, props }
    }

    test('shows a spinner before any locations emissions', () => {
        const { props } = getProps()
        expect(renderer.create(<HierarchicalLocationsView {...props} />).toJSON()).toMatchSnapshot()
    })

    test('shows a spinner if locations emits empty and is not complete', () => {
        const { props } = getProps()
        expect(
            renderer.create(<HierarchicalLocationsView {...props} locations={of(concat(of([]), NEVER))} />).toJSON()
        ).toMatchSnapshot()
    })

    test("registers a 'Group by file' contribution", () => {
        const { props, services } = getProps()
        renderer.create(<HierarchicalLocationsView {...props} />)
        expect(services.contribution.registerContributions.called).toBe(true)
        const expected: ContributionsEntry = {
            contributions: {
                actions: [
                    {
                        id: 'panel.locations.groupByFile',
                        title: parseTemplate('Group by file'),
                        category: parseTemplate('Locations (panel)'),
                        command: 'updateConfiguration',
                    },
                ],
                menus: {
                    'panel/toolbar': [
                        {
                            action: 'panel.locations.groupByFile',
                        },
                    ],
                },
            },
        }
        expect(services.contribution.registerContributions.getCall(0).args[0]).toMatchObject(expected)
    })

    const SAMPLE_LOCATION: Location = {
        uri: 'git://github.com/foo/bar',
        range: {
            start: {
                line: 1,
                character: 0,
            },
            end: {
                line: 1,
                character: 10,
            },
        },
    }

    test('displays a single location when complete', () => {
        const locations = of<Observable<Location[]>>(of([SAMPLE_LOCATION]))
        const props = {
            ...getProps().props,
            locations,
        }
        expect(renderer.create(<HierarchicalLocationsView {...props} />).toJSON()).toMatchSnapshot()
    })

    test('displays partial locations before complete', () => {
        const props = {
            ...getProps().props,
            locations: concat(of(of([SAMPLE_LOCATION])), NEVER),
        }
        expect(renderer.create(<HierarchicalLocationsView {...props} />).toJSON()).toMatchSnapshot()
    })

    test('displays multiple locations grouped by file', () => {
        const locations = of<Location[]>([
            {
                uri: 'git://github.com/foo/bar#file1.txt',
                range: {
                    start: {
                        line: 1,
                        character: 0,
                    },
                    end: {
                        line: 1,
                        character: 10,
                    },
                },
            },
            {
                uri: 'git://github.com/foo/bar#file2.txt',
                range: {
                    start: {
                        line: 2,
                        character: 0,
                    },
                    end: {
                        line: 2,
                        character: 10,
                    },
                },
            },
            {
                uri: 'git://github.com/foo/bar#file1.txt',
                range: {
                    start: {
                        line: 3,
                        character: 0,
                    },
                    end: {
                        line: 3,
                        character: 10,
                    },
                },
            },
            {
                uri: 'git://github.com/foo/bar#file2.txt',
                range: {
                    start: {
                        line: 4,
                        character: 0,
                    },
                    end: {
                        line: 4,
                        character: 10,
                    },
                },
            },
            {
                uri: 'git://github.com/foo/bar#file2.txt',
                range: {
                    start: {
                        line: 5,
                        character: 0,
                    },
                    end: {
                        line: 5,
                        character: 10,
                    },
                },
            },
        ])
        const props: HierarchicalLocationsViewProps = {
            ...getProps().props,
            settingsCascade: {
                subjects: null,
                final: {
                    'panel.locations.groupByFile': true,
                },
            },
            locations: of(locations),
        }
        expect(renderer.create(<HierarchicalLocationsView {...props} />).toJSON()).toMatchSnapshot()
    })
})
