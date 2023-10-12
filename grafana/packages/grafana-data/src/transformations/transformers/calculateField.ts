import { defaults } from 'lodash';
import { map } from 'rxjs/operators';

import { getTimeField } from '../../dataframe/processDataFrame';
import { getFieldDisplayName } from '../../field';
import { DataFrame, DataTransformerInfo, Field, FieldType, NullValueMode } from '../../types';
import { BinaryOperationID, binaryOperators } from '../../utils/binaryOperators';
import { UnaryOperationID, unaryOperators } from '../../utils/unaryOperators';
import { doStandardCalcs, fieldReducers, ReducerID } from '../fieldReducer';
import { getFieldMatcher } from '../matchers';
import { FieldMatcherID } from '../matchers/ids';

import { ensureColumnsTransformer } from './ensureColumns';
import { DataTransformerID } from './ids';
import { noopTransformer } from './noop';

export enum CalculateFieldMode {
  ReduceRow = 'reduceRow',
  BinaryOperation = 'binary',
  UnaryOperation = 'unary',
  Index = 'index',
}

export interface ReduceOptions {
  include?: string[]; // Assume all fields
  reducer: ReducerID;
  nullValueMode?: NullValueMode;
}

export interface UnaryOptions {
  operator: UnaryOperationID;
  fieldName: string;
}

export interface BinaryOptions {
  left: string;
  operator: BinaryOperationID;
  right: string;
}

export interface IndexOptions {
  asPercentile: boolean;
}

const defaultReduceOptions: ReduceOptions = {
  reducer: ReducerID.sum,
};

const defaultBinaryOptions: BinaryOptions = {
  left: '',
  operator: BinaryOperationID.Add,
  right: '',
};

const defaultUnaryOptions: UnaryOptions = {
  operator: UnaryOperationID.Abs,
  fieldName: '',
};

export interface CalculateFieldTransformerOptions {
  // True/False or auto
  timeSeries?: boolean;
  mode: CalculateFieldMode; // defaults to 'reduce'

  // Only one should be filled
  reduce?: ReduceOptions;
  binary?: BinaryOptions;
  unary?: UnaryOptions;
  index?: IndexOptions;

  // Remove other fields
  replaceFields?: boolean;

  // Output field properties
  alias?: string; // The output field name
  // TODO: config?: FieldConfig; or maybe field overrides? since the UI exists
}

type ValuesCreator = (data: DataFrame) => unknown[] | undefined;

export const calculateFieldTransformer: DataTransformerInfo<CalculateFieldTransformerOptions> = {
  id: DataTransformerID.calculateField,
  name: 'Add field from calculation',
  description: 'Use the row values to calculate a new field',
  defaultOptions: {
    mode: CalculateFieldMode.ReduceRow,
    reduce: {
      reducer: ReducerID.sum,
    },
  },
  operator: (options, ctx) => (outerSource) => {
    const operator =
      options && options.timeSeries !== false
        ? ensureColumnsTransformer.operator(null, ctx)
        : noopTransformer.operator({}, ctx);

    if (options.alias != null) {
      options.alias = ctx.interpolate(options.alias);
    }

    return outerSource.pipe(
      operator,
      map((data) => {
        const mode = options.mode ?? CalculateFieldMode.ReduceRow;
        let creator: ValuesCreator | undefined = undefined;

        if (mode === CalculateFieldMode.ReduceRow) {
          creator = getReduceRowCreator(defaults(options.reduce, defaultReduceOptions), data);
        } else if (mode === CalculateFieldMode.UnaryOperation) {
          creator = getUnaryCreator(defaults(options.unary, defaultUnaryOptions), data);
        } else if (mode === CalculateFieldMode.BinaryOperation) {
          const binaryOptions = {
            ...options.binary,
            left: ctx.interpolate(options.binary?.left!),
            right: ctx.interpolate(options.binary?.right!),
          };

          creator = getBinaryCreator(defaults(binaryOptions, defaultBinaryOptions), data);
        } else if (mode === CalculateFieldMode.Index) {
          return data.map((frame) => {
            const indexArr = [...Array(frame.length).keys()];

            if (options.index?.asPercentile) {
              for (let i = 0; i < indexArr.length; i++) {
                indexArr[i] = indexArr[i] / indexArr.length;
              }
            }

            const f = {
              name: options.alias ?? 'Row',
              type: FieldType.number,
              values: indexArr,
              config: options.index?.asPercentile ? { unit: 'percentunit' } : {},
            };
            return {
              ...frame,
              fields: options.replaceFields ? [f] : [...frame.fields, f],
            };
          });
        }

        // Nothing configured
        if (!creator) {
          return data;
        }

        return data.map((frame) => {
          // delegate field creation to the specific function
          const values = creator!(frame);
          if (!values) {
            return frame;
          }

          const field = {
            name: getNameFromOptions(options),
            type: FieldType.number,
            config: {},
            values,
          };
          let fields: Field[] = [];

          // Replace all fields with the single field
          if (options.replaceFields) {
            const { timeField } = getTimeField(frame);
            if (timeField && options.timeSeries !== false) {
              fields = [timeField, field];
            } else {
              fields = [field];
            }
          } else {
            fields = [...frame.fields, field];
          }
          return {
            ...frame,
            fields,
          };
        });
      })
    );
  },
};

function getReduceRowCreator(options: ReduceOptions, allFrames: DataFrame[]): ValuesCreator {
  let matcher = getFieldMatcher({
    id: FieldMatcherID.numeric,
  });

  if (options.include && options.include.length) {
    matcher = getFieldMatcher({
      id: FieldMatcherID.byNames,
      options: {
        names: options.include,
      },
    });
  }

  const info = fieldReducers.get(options.reducer);

  if (!info) {
    throw new Error(`Unknown reducer: ${options.reducer}`);
  }

  const reducer = info.reduce ?? doStandardCalcs;
  const ignoreNulls = options.nullValueMode === NullValueMode.Ignore;
  const nullAsZero = options.nullValueMode === NullValueMode.AsZero;

  return (frame: DataFrame) => {
    // Find the columns that should be examined
    const columns = [];
    for (const field of frame.fields) {
      if (matcher(field, frame, allFrames)) {
        columns.push(field.values);
      }
    }

    // Prepare a "fake" field for the row
    const size = columns.length;
    const row: Field = {
      name: 'temp',
      values: new Array(size),
      type: FieldType.number,
      config: {},
    };
    const vals: number[] = [];

    for (let i = 0; i < frame.length; i++) {
      for (let j = 0; j < size; j++) {
        row.values[j] = columns[j][i];
      }
      vals.push(reducer(row, ignoreNulls, nullAsZero)[options.reducer]);
    }

    return vals;
  };
}

function findFieldValuesWithNameOrConstant(
  frame: DataFrame,
  name: string,
  allFrames: DataFrame[]
): number[] | undefined {
  if (!name) {
    return undefined;
  }

  for (const f of frame.fields) {
    if (name === getFieldDisplayName(f, frame, allFrames)) {
      if (f.type === FieldType.boolean) {
        return f.values.map((v) => (v ? 1 : 0));
      }
      return f.values;
    }
  }

  const v = parseFloat(name);
  if (!isNaN(v)) {
    return new Array(frame.length).fill(v);
  }

  return undefined;
}

function getBinaryCreator(options: BinaryOptions, allFrames: DataFrame[]): ValuesCreator {
  const operator = binaryOperators.getIfExists(options.operator);

  return (frame: DataFrame) => {
    const left = findFieldValuesWithNameOrConstant(frame, options.left, allFrames);
    const right = findFieldValuesWithNameOrConstant(frame, options.right, allFrames);
    if (!left || !right || !operator) {
      return undefined;
    }

    const arr = new Array(left.length);
    for (let i = 0; i < arr.length; i++) {
      arr[i] = operator.operation(left[i], right[i]);
    }
    return arr;
  };
}

function getUnaryCreator(options: UnaryOptions, allFrames: DataFrame[]): ValuesCreator {
  const operator = unaryOperators.getIfExists(options.operator);

  return (frame: DataFrame) => {
    let value: number[] = [];

    for (const f of frame.fields) {
      if (options.fieldName === getFieldDisplayName(f, frame, allFrames) && f.type === FieldType.number) {
        value = f.values;
      }
    }

    if (!value.length || !operator) {
      return undefined;
    }

    const arr = new Array(value.length);
    for (let i = 0; i < arr.length; i++) {
      arr[i] = operator.operation(value[i]);
    }

    return arr;
  };
}

export function getNameFromOptions(options: CalculateFieldTransformerOptions) {
  if (options.alias?.length) {
    return options.alias;
  }

  switch (options.mode) {
    case CalculateFieldMode.UnaryOperation: {
      const { unary } = options;
      return `${unary?.operator ?? ''}${unary?.fieldName ? `(${unary.fieldName})` : ''}`;
    }
    case CalculateFieldMode.BinaryOperation: {
      const { binary } = options;
      const alias = `${binary?.left ?? ''} ${binary?.operator ?? ''} ${binary?.right ?? ''}`;

      //Remove $ signs as they will be interpolated and cause issues. Variables can still be used
      //in alias but shouldn't in the autogenerated name
      return alias.replace(/\$/g, '');
    }
    case CalculateFieldMode.ReduceRow:
      {
        const r = fieldReducers.getIfExists(options.reduce?.reducer);
        if (r) {
          return r.name;
        }
      }
      break;
    case CalculateFieldMode.Index:
      return 'Row';
  }

  return 'math';
}
