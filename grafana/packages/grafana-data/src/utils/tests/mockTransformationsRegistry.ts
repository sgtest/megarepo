import { standardTransformersRegistry } from '../../transformations/standardTransformersRegistry';
import { DataTransformerInfo } from '../../types';

export const mockTransformationsRegistry = (transformers: DataTransformerInfo[]) => {
  standardTransformersRegistry.setInit(() => {
    return transformers.map((t) => {
      return {
        id: t.id,
        aliasIds: t.aliasIds,
        name: t.name,
        transformation: t,
        description: t.description,
        editor: () => null,
      };
    });
  });
};
