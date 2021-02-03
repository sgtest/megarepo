/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.repositories.azure;

import static org.hamcrest.Matchers.blankOrNullString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.not;

import java.net.HttpURLConnection;
import java.util.Collection;

import org.elasticsearch.action.ActionRunnable;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.MockSecureSettings;
import org.elasticsearch.common.settings.SecureSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.repositories.AbstractThirdPartyRepositoryTestCase;
import org.elasticsearch.repositories.blobstore.BlobStoreRepository;

import com.azure.storage.blob.BlobContainerClient;
import com.azure.storage.blob.BlobServiceClient;
import com.azure.storage.blob.models.BlobStorageException;

public class AzureStorageCleanupThirdPartyTests extends AbstractThirdPartyRepositoryTestCase {

    @Override
    protected Collection<Class<? extends Plugin>> getPlugins() {
        return pluginList(AzureRepositoryPlugin.class);
    }

    @Override
    protected Settings nodeSettings() {
        final String endpoint = System.getProperty("test.azure.endpoint_suffix");
        if (Strings.hasText(endpoint)) {
            return Settings.builder()
                .put(super.nodeSettings())
                .put("azure.client.default.endpoint_suffix", endpoint)
                .build();
        }
        return super.nodeSettings();
    }

    @Override
    protected SecureSettings credentials() {
        assertThat(System.getProperty("test.azure.account"), not(blankOrNullString()));
        final boolean hasSasToken = Strings.hasText(System.getProperty("test.azure.sas_token"));
        if (hasSasToken == false) {
            assertThat(System.getProperty("test.azure.key"), not(blankOrNullString()));
        } else {
            assertThat(System.getProperty("test.azure.key"), blankOrNullString());
        }
        assertThat(System.getProperty("test.azure.container"), not(blankOrNullString()));
        assertThat(System.getProperty("test.azure.base"), not(blankOrNullString()));

        MockSecureSettings secureSettings = new MockSecureSettings();
        secureSettings.setString("azure.client.default.account", System.getProperty("test.azure.account"));
        if (hasSasToken) {
            secureSettings.setString("azure.client.default.sas_token", System.getProperty("test.azure.sas_token"));
        } else {
            secureSettings.setString("azure.client.default.key", System.getProperty("test.azure.key"));
        }
        return secureSettings;
    }

    @Override
    protected void createRepository(String repoName) {
        AcknowledgedResponse putRepositoryResponse = client().admin().cluster().preparePutRepository(repoName)
            .setType("azure")
            .setSettings(Settings.builder()
                .put("container", System.getProperty("test.azure.container"))
                .put("base_path", System.getProperty("test.azure.base"))
            ).get();
        assertThat(putRepositoryResponse.isAcknowledged(), equalTo(true));
        if (Strings.hasText(System.getProperty("test.azure.sas_token"))) {
            ensureSasTokenPermissions();
        }
    }

    private void ensureSasTokenPermissions() {
        final BlobStoreRepository repository = getRepository();
        final PlainActionFuture<Void> future = PlainActionFuture.newFuture();
        repository.threadPool().generic().execute(ActionRunnable.wrap(future, l -> {
            final AzureBlobStore blobStore = (AzureBlobStore) repository.blobStore();
            final AzureBlobServiceClient azureBlobServiceClient =
                blobStore.getService().client("default", LocationMode.PRIMARY_ONLY);
            final BlobServiceClient client = azureBlobServiceClient.getSyncClient();
            try {
                SocketAccess.doPrivilegedException(() -> {
                    final BlobContainerClient blobContainer = client.getBlobContainerClient(blobStore.toString());
                    return blobContainer.exists();
                });
                future.onFailure(new RuntimeException(
                    "The SAS token used in this test allowed for checking container existence. This test only supports tokens " +
                        "that grant only the documented permission requirements for the Azure repository plugin."));
            } catch (BlobStorageException e) {
                if (e.getStatusCode() == HttpURLConnection.HTTP_FORBIDDEN) {
                    future.onResponse(null);
                } else {
                    future.onFailure(e);
                }
            }
        }));
        future.actionGet();
    }
}
