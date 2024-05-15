package goreman

import (
	"net"
	"net/rpc"
)

type Goreman struct{}

// rpc: restart all (stop all, then start all)
func (Goreman) RestartAll(args struct{}, ret *string) (err error) {
	defer func() {
		if r := recover(); r != nil {
			err = r.(error)
		}
	}()

	// Stop and start the processes. We do this with an artificially
	// incremented wg, so that the server does not shutdown when stopProcs
	// completes (the server shuts down when all processes are stopped).
	//
	// Note that we do not invoke waitProcs, as the original waitProcs
	// invocation is still running and is still valid.
	wg.Add(1)
	defer wg.Done()
	stopProcs(false)
	startProcs()

	return nil
}

// start rpc server.
func startServer(addr string) error {
	rpc.Register(Goreman{})
	server, err := net.Listen("tcp", addr)
	if err != nil {
		return err
	}
	go func() {
		for {
			client, err := server.Accept()
			if err != nil {
				continue
			}
			rpc.ServeConn(client)
		}
	}()
	return nil
}
