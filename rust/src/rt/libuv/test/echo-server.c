/* Copyright Joyent, Inc. and other Node contributors. All rights reserved.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to
 * deal in the Software without restriction, including without limitation the
 * rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
 * sell copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
 * FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
 * IN THE SOFTWARE.
 */

#include "uv.h"
#include "task.h"
#include <stdio.h>
#include <stdlib.h>


typedef struct {
  uv_req_t req;
  uv_buf_t buf;
} write_req_t;


static int server_closed;
static uv_tcp_t server;

static int server6_closed;
static uv_tcp_t server6;


static void after_write(uv_req_t* req, int status);
static void after_read(uv_stream_t*, ssize_t nread, uv_buf_t buf);
static void on_close(uv_handle_t* peer);
static void on_server_close(uv_handle_t* handle);
static void on_connection(uv_handle_t*, int status);


static void after_write(uv_req_t* req, int status) {
  write_req_t* wr;

  if (status) {
    uv_err_t err = uv_last_error();
    fprintf(stderr, "uv_write error: %s\n", uv_strerror(err));
    ASSERT(0);
  }

  wr = (write_req_t*) req;

  /* Free the read/write buffer and the request */
  free(wr->buf.base);
  free(wr);
}


static void after_shutdown(uv_req_t* req, int status) {
  uv_close(req->handle, on_close);
  free(req);
}


static void after_read(uv_stream_t* handle, ssize_t nread, uv_buf_t buf) {
  int i;
  write_req_t *wr;
  uv_req_t* req;

  if (nread < 0) {
    /* Error or EOF */
    ASSERT (uv_last_error().code == UV_EOF);

    if (buf.base) {
      free(buf.base);
    }

    req = (uv_req_t*) malloc(sizeof *req);
    uv_req_init(req, (uv_handle_t*)handle, (void *(*)(void *))after_shutdown);
    uv_shutdown(req);

    return;
  }

  if (nread == 0) {
    /* Everything OK, but nothing read. */
    free(buf.base);
    return;
  }

  /* Scan for the letter Q which signals that we should quit. */
  if (!server_closed) {
    for (i = 0; i < nread; i++) {
      if (buf.base[i] == 'Q') {
        uv_close((uv_handle_t*)&server, on_server_close);
        server_closed = 1;
        uv_close((uv_handle_t*)&server6, on_server_close);
        server6_closed = 1;
      }
    }
  }

  wr = (write_req_t*) malloc(sizeof *wr);

  uv_req_init(&wr->req, (uv_handle_t*)handle, (void *(*)(void *))after_write);
  wr->buf.base = buf.base;
  wr->buf.len = nread;
  if (uv_write(&wr->req, &wr->buf, 1)) {
    FATAL("uv_write failed");
  }
}


static void on_close(uv_handle_t* peer) {
  free(peer);
}


static uv_buf_t echo_alloc(uv_stream_t* handle, size_t suggested_size) {
  uv_buf_t buf;
  buf.base = (char*) malloc(suggested_size);
  buf.len = suggested_size;
  return buf;
}


static void on_connection(uv_handle_t* server, int status) {
  uv_tcp_t* handle;
  int r;

  if (status != 0) {
    fprintf(stderr, "Connect error %d\n", uv_last_error().code);
  }
  ASSERT(status == 0);

  handle = (uv_tcp_t*) malloc(sizeof *handle);
  ASSERT(handle != NULL);

  uv_tcp_init(handle);

  /* associate server with stream */
  handle->data = server;

  r = uv_accept(server, (uv_stream_t*)handle);
  ASSERT(r == 0);

  r = uv_read_start((uv_stream_t*)handle, echo_alloc, after_read);
  ASSERT(r == 0);
}


static void on_server_close(uv_handle_t* handle) {
  ASSERT(handle == (uv_handle_t*)&server || handle == (uv_handle_t*)&server6);
}


static int echo_start(int port) {
  struct sockaddr_in addr = uv_ip4_addr("0.0.0.0", port);
  struct sockaddr_in6 addr6 = uv_ip6_addr("::1", port);
  int r;

  r = uv_tcp_init(&server);
  if (r) {
    /* TODO: Error codes */
    fprintf(stderr, "Socket creation error\n");
    return 1;
  }

  r = uv_tcp_bind(&server, addr);
  if (r) {
    /* TODO: Error codes */
    fprintf(stderr, "Bind error\n");
    return 1;
  }

  r = uv_tcp_listen(&server, 128, on_connection);
  if (r) {
    /* TODO: Error codes */
    fprintf(stderr, "Listen error\n");
    return 1;
  }

  r = uv_tcp_init(&server6);
  if (r) {
    /* TODO: Error codes */
    fprintf(stderr, "Socket creation error\n");
    return 1;
  }

  /* IPv6 is optional as not all platforms support it */
  r = uv_tcp_bind6(&server6, addr6);
  if (r) {
    /* show message but return OK */
    fprintf(stderr, "IPv6 not supported\n");
    return 0;
  }

  r = uv_tcp_listen(&server6, 128, on_connection);
  if (r) {
    /* TODO: Error codes */
    fprintf(stderr, "Listen error on IPv6\n");
    return 1;
  }

  return 0;
}


HELPER_IMPL(echo_server) {
  uv_init();
  if (echo_start(TEST_PORT))
    return 1;

  uv_run();
  return 0;
}
