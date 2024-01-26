import socket
import encode

# RemoteCollector is a remote collector that operates over a standard, and
# synchronous, Python socket.
#
#  rc = RemoteCollector(sock=None, debug=True)
#
#  try:
#      rc.connect(host="localhost", port=7701)
#  except Exception as e:
#      print "Failed to connect:", e
#
#  try:
#      rc.collect(spanID, annotationOne, annotationTwo)
#  except Exception as e:
#      print "Failed to collect:", e
#
#  rc.close()
#
# A custom socket for the sock parameter to RemoteCollector's allows one to
# specify e.g. a TLS socket.
class RemoteCollector:
    # sock is literally the socket that is used to communicate with the remote
    # collector.
    sock = None

    _debug = False

    def __init__(self, sock=None, debug=False):
        self.sock = sock
        self._debug = debug

    def _log(self, *args):
        if self._debug:
            print "appdash: %s" % (" ".join(args))

    # connect connects the underlying socket to the given address, waiting at
    # max for the given timeout before raising an exception.
    def connect(self, host="localhost", port=7701, timeout=10):
        # Use the given socket, or create a new one.
        if self.sock is None:
            self.sock = socket.create_connection((host, port), timeout=timeout)
        else:
            self.sock.connect()

    # collect collects annotations for the given spanID.
    #
    # The annotations are sent to the remote server immediately, and this
    # function does not return until all have been sent out or an exception has
    # occured (e.g. disconnection).
    def collect(self, spanID, *annotations):
        self._log("collecting", str(len(annotations)), "annotations for", str(spanID))
        packet = encode._collect(spanID, *annotations)
        buf = encode._msg(packet)

        totalSent = 0
        while totalSent < len(buf):
            sent = self.sock.send(buf[totalSent:])
            if sent == 0:
                raise RuntimeError("socket connection broken")
            totalSent = totalSent + sent

    # close closes the underlying socket.
    def close(self):
        self.sock.close()
        self.sock = None

