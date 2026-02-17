jest.mock('@posthog/hogvm', () => ({}), { virtual: true })

global.fetch = jest.fn().mockImplementation(() => Promise.resolve({ ok: true, json: () => Promise.resolve({}) }))

import React from 'react'

import { kea, key } from 'kea'
import api from 'lib/api'

import { useMocks } from '~/mocks/jest'
import { DataVisualizationNode, NodeKind } from '~/queries/schema/schema-general'
import { initKeaTests } from '~/test/init'

const dataNodeLogicMock = jest.fn()

jest.mock('~/queries/nodes/DataNode/dataNodeLogic', () => {
    const { kea, key } = require('kea')
    const dataNodeLogic = (props: any) => {
        const wrapper = kea([
            key((p: any) => p.key),
            {
                actions: () => ({
                    loadData: () => ({}),
                }),
                reducers: () => ({
                    response: [null],
                    responseLoading: [false],
                    responseError: [null],
                    queryCancelled: [false],
                }),
            },
        ])
        const built = wrapper(props)
        dataNodeLogicMock(props)
        return built
    }
    return { dataNodeLogic }
})

jest.mock('~/layout/navigation-3000/themeLogic', () => {
    const { kea } = require('kea')
    return {
        themeLogic: kea({
            selectors: {
                isDarkModeOn: [() => [], () => false],
            },
        }),
    }
})

jest.mock('scenes/teamLogic', () => {
    const { kea } = require('kea')
    return {
        teamLogic: kea({
            selectors: {
                currentTeamId: [() => [], () => 1],
            },
        }),
    }
})

jest.mock('scenes/sceneLogic', () => {
    const { kea } = require('kea')
    return {
        sceneLogic: kea({
            selectors: {
                activeSceneId: [() => [], () => null],
            },
        }),
    }
})

jest.mock('kea-router', () => {
    const actual = jest.requireActual('kea-router')
    return {
        ...actual,
        router: {
            actions: {
                replace: jest.fn(),
            },
        },
    }
})

afterEach(() => {
    jest.clearAllMocks()
})

const { dataVisualizationLogic } = require('~/queries/nodes/DataVisualization/dataVisualizationLogic')

const testQuery: DataVisualizationNode = {
    kind: NodeKind.DataVisualizationNode,
    source: {
        kind: NodeKind.HogQLQuery,
        query: 'select event, count() as event_count from events group by event',
    },
    tableSettings: {
        columns: [],
        conditionalFormatting: [],
    },
    chartSettings: { goalLines: undefined },
}

describe('dataVisualizationLogic limitContext forwarding', () => {
    beforeEach(() => {
        useMocks({})
        initKeaTests()
        Object.defineProperty(window, 'matchMedia', {
            writable: true,
            value: jest.fn().mockImplementation((query) => ({
                matches: false,
                media: query,
                onchange: null,
                addEventListener: jest.fn(),
                removeEventListener: jest.fn(),
                addListener: jest.fn(),
                removeListener: jest.fn(),
                dispatchEvent: jest.fn(),
            })),
        })
    })

    it('passes limitContext into dataNodeLogic connections', () => {
        dataNodeLogicMock.mockClear()

        const logic = dataVisualizationLogic({
            key: 'data-viz-limit-test',
            query: testQuery,
            dataNodeCollectionId: 'test-collection',
            limitContext: 'posthog_ai',
        })
        logic.mount()

        expect(dataNodeLogicMock).toHaveBeenCalled()
        const propsCalls = dataNodeLogicMock.mock.calls.map((call) => call[0])
        const limitContexts = propsCalls.map((props) => props.limitContext)
        expect(limitContexts).toContain('posthog_ai')
        expect(propsCalls.every((props) => props.query === testQuery.source)).toBe(true)

        logic.unmount()
    })

    it('skips limitContext when not provided', () => {
        dataNodeLogicMock.mockClear()

        const logic = dataVisualizationLogic({
            key: 'data-viz-no-limit-test',
            query: testQuery,
            dataNodeCollectionId: 'test-collection',
        })
        logic.mount()

        expect(dataNodeLogicMock).toHaveBeenCalled()
        const propsCalls = dataNodeLogicMock.mock.calls.map((call) => call[0])
        expect(propsCalls.every((props) => props.limitContext === undefined)).toBe(true)

        logic.unmount()
    })

    it('still triggers loadData without limitContext provided', async () => {
        const querySpy = jest
            .spyOn(api, 'query')
            .mockResolvedValue({ columns: ['event'], results: [['signup']], hasMore: false })

        const logic = dataVisualizationLogic({
            key: 'data-viz-load-test',
            query: testQuery,
            dataNodeCollectionId: 'test-collection',
        })
        logic.mount()

        await logic.actions.loadData()

        expect(querySpy).not.toHaveBeenCalled()
        logic.unmount()
    })
})
