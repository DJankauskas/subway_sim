import z from 'zod';

export const SubwayGraph = z.object({
  nodes: z.array(z.object({
      id: z.string(),
      position: z.object({
          x: z.number(),
          y: z.number(),
      })
  })),
  edges: z.array(z.object({
      id: z.string(),
      source: z.string(),
      target: z.string(),
      weight: z.number(),
  })),
});
export type SubwayGraph = z.infer<typeof SubwayGraph>;

export function defaultSubwayGraph(): SubwayGraph {
    return {
        nodes: [],
        edges: [],
    }
}