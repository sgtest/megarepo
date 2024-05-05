use std::io;

use crate::shims::unix::*;
use crate::*;

use self::fd::FileDescriptor;

/// Pair of connected sockets.
///
/// We currently don't allow sending any data through this pair, so this can be just a dummy.
#[derive(Debug)]
struct SocketPair;

impl FileDescription for SocketPair {
    fn name(&self) -> &'static str {
        "socketpair"
    }

    fn close<'tcx>(
        self: Box<Self>,
        _communicate_allowed: bool,
    ) -> InterpResult<'tcx, io::Result<()>> {
        Ok(Ok(()))
    }
}

impl<'mir, 'tcx: 'mir> EvalContextExt<'mir, 'tcx> for crate::MiriInterpCx<'mir, 'tcx> {}
pub trait EvalContextExt<'mir, 'tcx: 'mir>: crate::MiriInterpCxExt<'mir, 'tcx> {
    /// Currently this function this function is a stub. Eventually we need to
    /// properly implement an FD type for sockets and have this function create
    /// two sockets and associated FDs such that writing to one will produce
    /// data that can be read from the other.
    ///
    /// For more information on the arguments see the socketpair manpage:
    /// <https://linux.die.net/man/2/socketpair>
    fn socketpair(
        &mut self,
        domain: &OpTy<'tcx, Provenance>,
        type_: &OpTy<'tcx, Provenance>,
        protocol: &OpTy<'tcx, Provenance>,
        sv: &OpTy<'tcx, Provenance>,
    ) -> InterpResult<'tcx, Scalar<Provenance>> {
        let this = self.eval_context_mut();

        let _domain = this.read_scalar(domain)?.to_i32()?;
        let _type_ = this.read_scalar(type_)?.to_i32()?;
        let _protocol = this.read_scalar(protocol)?.to_i32()?;
        let sv = this.deref_pointer(sv)?;

        // FIXME: fail on unsupported inputs

        let fds = &mut this.machine.fds;
        let sv0 = fds.insert_fd(FileDescriptor::new(SocketPair));
        let sv0 = Scalar::try_from_int(sv0, sv.layout.size).unwrap();
        let sv1 = fds.insert_fd(FileDescriptor::new(SocketPair));
        let sv1 = Scalar::try_from_int(sv1, sv.layout.size).unwrap();

        this.write_scalar(sv0, &sv)?;
        this.write_scalar(sv1, &sv.offset(sv.layout.size, sv.layout, this)?)?;

        Ok(Scalar::from_i32(0))
    }
}
