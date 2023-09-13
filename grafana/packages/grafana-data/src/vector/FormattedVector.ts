import { DisplayProcessor } from '../types';
import { Vector } from '../types/vector';
import { formattedValueToString } from '../valueFormats';

/**
 * @public
 * @deprecated use a simple Arrays. NOTE: not used in grafana core.
 */
export class FormattedVector<T = any> extends Array<string> {
  constructor(source: Vector<T>, formatter: DisplayProcessor) {
    super();
    return source.map((v) => formattedValueToString(formatter(v)));
  }
}
