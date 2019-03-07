/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.transport.nio;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.network.NetworkService;
import org.elasticsearch.common.recycler.Recycler;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.PageCacheRecycler;
import org.elasticsearch.indices.breaker.CircuitBreakerService;
import org.elasticsearch.nio.BytesChannelContext;
import org.elasticsearch.nio.ChannelFactory;
import org.elasticsearch.nio.InboundChannelBuffer;
import org.elasticsearch.nio.NioSelector;
import org.elasticsearch.nio.NioSocketChannel;
import org.elasticsearch.nio.ServerChannelContext;
import org.elasticsearch.nio.SocketChannelContext;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.ConnectTransportException;
import org.elasticsearch.transport.TcpChannel;
import org.elasticsearch.transport.TransportSettings;
import org.elasticsearch.transport.nio.NioGroupFactory;
import org.elasticsearch.transport.nio.NioTcpChannel;
import org.elasticsearch.transport.nio.NioTcpServerChannel;
import org.elasticsearch.transport.nio.NioTransport;
import org.elasticsearch.transport.nio.TcpReadWriteHandler;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.security.transport.ProfileConfigurations;
import org.elasticsearch.xpack.core.security.transport.SecurityTransportExceptionHandler;
import org.elasticsearch.xpack.core.ssl.SSLConfiguration;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.security.transport.filter.IPFilter;

import javax.net.ssl.SNIHostName;
import javax.net.ssl.SSLEngine;
import javax.net.ssl.SSLParameters;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.util.Collections;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Supplier;

import static org.elasticsearch.xpack.core.security.SecurityField.setting;

/**
 * This transport provides a transport based on nio that is secured by SSL/TLS. SSL/TLS is a communications
 * protocol that allows two channels to go through a handshake process prior to application data being
 * exchanged. The handshake process enables the channels to exchange parameters that will allow them to
 * encrypt the application data they exchange.
 * <p>
 * The specific SSL/TLS parameters and configurations are setup in the {@link SSLService} class. The actual
 * implementation of the SSL/TLS layer is in the {@link SSLChannelContext} and {@link SSLDriver} classes.
 */
public class SecurityNioTransport extends NioTransport {
    private static final Logger logger = LogManager.getLogger(SecurityNioTransport.class);

    private final SecurityTransportExceptionHandler exceptionHandler;
    private final IPFilter authenticator;
    private final SSLService sslService;
    private final Map<String, SSLConfiguration> profileConfiguration;
    private final boolean sslEnabled;

    public SecurityNioTransport(Settings settings, Version version, ThreadPool threadPool, NetworkService networkService,
                                PageCacheRecycler pageCacheRecycler, NamedWriteableRegistry namedWriteableRegistry,
                                CircuitBreakerService circuitBreakerService, @Nullable final IPFilter authenticator,
                                SSLService sslService, NioGroupFactory groupFactory) {
        super(settings, version, threadPool, networkService, pageCacheRecycler, namedWriteableRegistry, circuitBreakerService,
            groupFactory);
        this.exceptionHandler = new SecurityTransportExceptionHandler(logger, lifecycle, (c, e) -> super.onException(c, e));
        this.authenticator = authenticator;
        this.sslService = sslService;
        this.sslEnabled = XPackSettings.TRANSPORT_SSL_ENABLED.get(settings);
        if (sslEnabled) {
            final SSLConfiguration transportConfiguration = sslService.getSSLConfiguration(setting("transport.ssl."));
            Map<String, SSLConfiguration> profileConfiguration = ProfileConfigurations.get(settings, sslService, transportConfiguration);
            this.profileConfiguration = Collections.unmodifiableMap(profileConfiguration);
        } else {
            profileConfiguration = Collections.emptyMap();
        }
    }

    @Override
    protected void doStart() {
        super.doStart();
        if (authenticator != null) {
            authenticator.setBoundTransportAddress(boundAddress(), profileBoundAddresses());
        }
    }

    @Override
    public void onException(TcpChannel channel, Exception e) {
        exceptionHandler.accept(channel, e);
    }

    @Override
    protected TcpChannelFactory serverChannelFactory(ProfileSettings profileSettings) {
        return new SecurityTcpChannelFactory(profileSettings, false);
    }

    @Override
    protected Function<DiscoveryNode, TcpChannelFactory> clientChannelFactoryFunction(ProfileSettings profileSettings) {
        return (node) -> {
            final ChannelFactory.RawChannelFactory rawChannelFactory = new ChannelFactory.RawChannelFactory(profileSettings.tcpNoDelay,
                profileSettings.tcpKeepAlive, profileSettings.reuseAddress, Math.toIntExact(profileSettings.sendBufferSize.getBytes()),
                Math.toIntExact(profileSettings.receiveBufferSize.getBytes()));
            SNIHostName serverName;
            String configuredServerName = node.getAttributes().get("server_name");
            if (configuredServerName != null) {
                try {
                    serverName = new SNIHostName(configuredServerName);
                } catch (IllegalArgumentException e) {
                    throw new ConnectTransportException(node, "invalid DiscoveryNode server_name [" + configuredServerName + "]", e);
                }
            } else {
                serverName = null;
            }
            return new SecurityClientTcpChannelFactory(rawChannelFactory, serverName);
        };
    }

    private class SecurityTcpChannelFactory extends TcpChannelFactory {

        private final String profileName;
        private final boolean isClient;
        private final NioIPFilter ipFilter;

        private SecurityTcpChannelFactory(ProfileSettings profileSettings, boolean isClient) {
            this(new RawChannelFactory(profileSettings.tcpNoDelay,
                profileSettings.tcpKeepAlive,
                profileSettings.reuseAddress,
                Math.toIntExact(profileSettings.sendBufferSize.getBytes()),
                Math.toIntExact(profileSettings.receiveBufferSize.getBytes())), profileSettings.profileName, isClient);
        }

        private SecurityTcpChannelFactory(RawChannelFactory rawChannelFactory, String profileName, boolean isClient) {
            super(rawChannelFactory);
            this.profileName = profileName;
            this.isClient = isClient;
            this.ipFilter = new NioIPFilter(authenticator, profileName);
        }

        @Override
        public NioTcpChannel createChannel(NioSelector selector, SocketChannel channel) throws IOException {
            NioTcpChannel nioChannel = new NioTcpChannel(isClient == false, profileName, channel);
            Supplier<InboundChannelBuffer.Page> pageSupplier = () -> {
                Recycler.V<byte[]> bytes = pageCacheRecycler.bytePage(false);
                return new InboundChannelBuffer.Page(ByteBuffer.wrap(bytes.v()), bytes::close);
            };
            TcpReadWriteHandler readWriteHandler = new TcpReadWriteHandler(nioChannel, SecurityNioTransport.this);
            InboundChannelBuffer buffer = new InboundChannelBuffer(pageSupplier);
            Consumer<Exception> exceptionHandler = (e) -> onException(nioChannel, e);

            SocketChannelContext context;
            if (sslEnabled) {
                SSLDriver sslDriver = new SSLDriver(createSSLEngine(channel), isClient);
                context = new SSLChannelContext(nioChannel, selector, exceptionHandler, sslDriver, readWriteHandler, buffer, ipFilter);
            } else {
                context = new BytesChannelContext(nioChannel, selector, exceptionHandler, readWriteHandler, buffer, ipFilter);
            }
            nioChannel.setContext(context);

            return nioChannel;
        }

        @Override
        public NioTcpServerChannel createServerChannel(NioSelector selector, ServerSocketChannel channel) throws IOException {
            NioTcpServerChannel nioChannel = new NioTcpServerChannel(profileName, channel);
            Consumer<Exception> exceptionHandler = (e) -> onServerException(nioChannel, e);
            Consumer<NioSocketChannel> acceptor = SecurityNioTransport.this::acceptChannel;
            ServerChannelContext context = new ServerChannelContext(nioChannel, this, selector, acceptor, exceptionHandler);
            nioChannel.setContext(context);
            return nioChannel;
        }

        protected SSLEngine createSSLEngine(SocketChannel channel) throws IOException {
            SSLEngine sslEngine;
            SSLConfiguration defaultConfig = profileConfiguration.get(TransportSettings.DEFAULT_PROFILE);
            SSLConfiguration sslConfig = profileConfiguration.getOrDefault(profileName, defaultConfig);
            boolean hostnameVerificationEnabled = sslConfig.verificationMode().isHostnameVerificationEnabled();
            if (hostnameVerificationEnabled) {
                InetSocketAddress inetSocketAddress = (InetSocketAddress) channel.getRemoteAddress();
                // we create the socket based on the name given. don't reverse DNS
                sslEngine = sslService.createSSLEngine(sslConfig, inetSocketAddress.getHostString(), inetSocketAddress.getPort());
            } else {
                sslEngine = sslService.createSSLEngine(sslConfig, null, -1);
            }
            return sslEngine;
        }
    }

    private class SecurityClientTcpChannelFactory extends SecurityTcpChannelFactory {

        private final SNIHostName serverName;

        private SecurityClientTcpChannelFactory(RawChannelFactory rawChannelFactory, SNIHostName serverName) {
            super(rawChannelFactory, TransportSettings.DEFAULT_PROFILE, true);
            this.serverName = serverName;
        }

        @Override
        public NioTcpServerChannel createServerChannel(NioSelector selector, ServerSocketChannel channel) {
            throw new AssertionError("Cannot create TcpServerChannel with client factory");
        }

        @Override
        protected SSLEngine createSSLEngine(SocketChannel channel) throws IOException {
            SSLEngine sslEngine = super.createSSLEngine(channel);
            if (serverName != null) {
                SSLParameters sslParameters = sslEngine.getSSLParameters();
                sslParameters.setServerNames(Collections.singletonList(serverName));
                sslEngine.setSSLParameters(sslParameters);
            }
            return sslEngine;
        }
    }
}
