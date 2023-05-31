import { BaseQueryFn, createApi } from '@reduxjs/toolkit/query/react';
import { lastValueFrom } from 'rxjs';

import { isTruthy } from '@grafana/data';
import { BackendSrvRequest, getBackendSrv } from '@grafana/runtime';
import { DescendantCount, DescendantCountDTO, FolderDTO } from 'app/types';

import { DashboardTreeSelection } from '../types';

interface RequestOptions extends BackendSrvRequest {
  manageError?: (err: unknown) => { error: unknown };
  showErrorAlert?: boolean;
}

function createBackendSrvBaseQuery({ baseURL }: { baseURL: string }): BaseQueryFn<RequestOptions> {
  async function backendSrvBaseQuery(requestOptions: RequestOptions) {
    try {
      const { data: responseData, ...meta } = await lastValueFrom(
        getBackendSrv().fetch({
          ...requestOptions,
          url: baseURL + requestOptions.url,
          showErrorAlert: requestOptions.showErrorAlert,
        })
      );
      return { data: responseData, meta };
    } catch (error) {
      return requestOptions.manageError ? requestOptions.manageError(error) : { error };
    }
  }

  return backendSrvBaseQuery;
}

export const browseDashboardsAPI = createApi({
  tagTypes: ['getFolder'],
  reducerPath: 'browseDashboardsAPI',
  baseQuery: createBackendSrvBaseQuery({ baseURL: '/api' }),
  endpoints: (builder) => ({
    getFolder: builder.query<FolderDTO, string>({
      query: (folderUID) => ({ url: `/folders/${folderUID}`, params: { accesscontrol: true } }),
      providesTags: (_result, _error, arg) => [{ type: 'getFolder', id: arg }],
    }),
    saveFolder: builder.mutation<FolderDTO, FolderDTO>({
      invalidatesTags: (_result, _error, args) => [{ type: 'getFolder', id: args.uid }],
      query: (folder) => ({
        method: 'PUT',
        showErrorAlert: false,
        url: `/folders/${folder.uid}`,
        data: {
          title: folder.title,
          version: folder.version,
        },
      }),
    }),
    getAffectedItems: builder.query<DescendantCount, DashboardTreeSelection>({
      queryFn: async (selectedItems) => {
        const folderUIDs = Object.keys(selectedItems.folder).filter((uid) => selectedItems.folder[uid]);

        const promises = folderUIDs.map((folderUID) => {
          return getBackendSrv().get<DescendantCountDTO>(`/api/folders/${folderUID}/counts`);
        });

        const results = await Promise.all(promises);

        const totalCounts = {
          folder: Object.values(selectedItems.folder).filter(isTruthy).length,
          dashboard: Object.values(selectedItems.dashboard).filter(isTruthy).length,
          libraryPanel: 0,
          alertRule: 0,
        };

        for (const folderCounts of results) {
          totalCounts.folder += folderCounts.folder;
          totalCounts.dashboard += folderCounts.dashboard;
          totalCounts.alertRule += folderCounts.alertrule ?? 0;

          // TODO enable these once the backend correctly returns them
          // totalCounts.libraryPanel += folderCounts.libraryPanel;
        }

        return { data: totalCounts };
      },
    }),
    moveFolder: builder.mutation<void, { folderUID: string; destinationUID: string }>({
      query: ({ folderUID, destinationUID }) => ({
        url: `/folders/${folderUID}/move`,
        method: 'POST',
        data: { parentUID: destinationUID },
      }),
      invalidatesTags: (_result, _error, arg) => [{ type: 'getFolder', id: arg.folderUID }],
    }),
  }),
});

export const { endpoints, useGetAffectedItemsQuery, useGetFolderQuery, useMoveFolderMutation, useSaveFolderMutation } =
  browseDashboardsAPI;
export { skipToken } from '@reduxjs/toolkit/query/react';
