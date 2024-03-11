/* Copyright 2024 The OpenXLA Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
==============================================================================*/

#include "xla/python/nb_numpy.h"

#include <Python.h>

#include <cstdint>
#include <stdexcept>

#include "absl/types/span.h"
#include "third_party/nanobind/include/nanobind/nanobind.h"
#include "tsl/python/lib/core/numpy.h"  // NOLINT

namespace nb = nanobind;

namespace xla {

/*static*/ nb_dtype nb_dtype::from_args(const nb::object& args) {
  PyArray_Descr* descr;
  if (!PyArray_DescrConverter(args.ptr(), &descr) || !descr) {
    throw nb::python_error();
  }
  return nb::steal<nb_dtype>(reinterpret_cast<PyObject*>(descr));
}

nb_numpy_ndarray::nb_numpy_ndarray(nb_dtype dtype,
                                   absl::Span<int64_t const> shape,
                                   absl::Span<int64_t const> strides,
                                   const void* ptr, nb::handle base) {
  if (shape.size() != strides.size()) {
    throw std::invalid_argument("shape and strides must have the same size.");
  }
  int flags = 0;
  if (base && ptr) {
    nb_numpy_ndarray base_array;
    if (nb::try_cast<nb_numpy_ndarray>(base, base_array)) {
      flags = base_array.flags() & ~NPY_ARRAY_OWNDATA;
    } else {
      flags = NPY_ARRAY_WRITEABLE;
    }
  }
  // The reinterpret_cast below assumes that ssize_t and int64_t are the same
  // width. If that changes, then the code should be updated to convert instead.
  static_assert(sizeof(int64_t) == sizeof(ssize_t));
  nb::object array = nb::steal<nb::object>(PyArray_NewFromDescr(
      &PyArray_Type, reinterpret_cast<PyArray_Descr*>(dtype.release().ptr()),
      shape.size(), reinterpret_cast<const ssize_t*>(shape.data()),
      reinterpret_cast<const ssize_t*>(strides.data()), const_cast<void*>(ptr),
      flags,
      /*obj=*/nullptr));
  if (!array) {
    throw nb::python_error();
  }
  if (ptr) {
    if (base) {
      PyArray_SetBaseObject(reinterpret_cast<PyArrayObject*>(array.ptr()),
                            base.inc_ref().ptr());
    } else {
      array = nb::steal<nb::object>(PyArray_NewCopy(
          reinterpret_cast<PyArrayObject*>(array.ptr()), NPY_ANYORDER));
    }
  }
  m_ptr = array.release().ptr();
}

/*static*/ nb_numpy_ndarray nb_numpy_ndarray::ensure(nanobind::handle h,
                                                     int extra_requirements) {
  nb::handle out = PyArray_FromAny(
      h.ptr(), /*dtype=*/nullptr, /*min_depth=*/0,
      /*max_depth=*/0,
      /*requirements=*/NPY_ARRAY_ENSUREARRAY | extra_requirements,
      /*context=*/nullptr);
  if (!out) {
    PyErr_Clear();
  }
  return nb::steal<nb_numpy_ndarray>(out);
}

nb_dtype nb_numpy_ndarray::dtype() const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  return nb::borrow<nb_dtype>(reinterpret_cast<PyObject*>(PyArray_DESCR(self)));
}

ssize_t nb_numpy_ndarray::ndim() const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  return PyArray_NDIM(self);
}

const ssize_t* nb_numpy_ndarray::shape() const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  return PyArray_SHAPE(self);
}

ssize_t nb_numpy_ndarray::shape(ssize_t dim) const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  if (dim < 0 || dim >= PyArray_NDIM(self)) {
    throw std::invalid_argument("Invalid dimension.");
  }
  return PyArray_SHAPE(self)[dim];
}

const ssize_t* nb_numpy_ndarray::strides() const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  return PyArray_STRIDES(self);
}

ssize_t nb_numpy_ndarray::strides(ssize_t dim) const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  if (dim < 0 || dim >= PyArray_NDIM(self)) {
    throw std::invalid_argument("Invalid dimension.");
  }
  return PyArray_STRIDES(self)[dim];
}

ssize_t nb_numpy_ndarray::itemsize() const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  return PyArray_ITEMSIZE(self);
}

ssize_t nb_numpy_ndarray::size() const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  return PyArray_SIZE(self);
}

const void* nb_numpy_ndarray::data() const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  return PyArray_DATA(self);
}

int nb_numpy_ndarray::flags() const {
  PyArrayObject* self = reinterpret_cast<PyArrayObject*>(ptr());
  return PyArray_FLAGS(self);
}

}  // namespace xla
