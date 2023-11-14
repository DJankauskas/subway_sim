import z from 'zod';

export const Route = z.object({
    name: z.string(),
    id: z.string(),
    nodes: z.array(z.string()),
    edges: z.array(z.string()),
});

export type Route = z.infer<typeof Route>;

export const SubwayGraph = z.object({
    nodes: z.array(z.object({
        id: z.string(),
        name: z.string(),
        position: z.object({
            x: z.number(),
            y: z.number(),
        })
    })),
    edges: z.array(z.object({
        id: z.string(),
        type: z.enum(["walk", "track"]),
        source: z.string(),
        target: z.string(),
        weight: z.number(),
    })),
    routes: z.record(z.string(), Route)
});
export type SubwayGraph = z.infer<typeof SubwayGraph>;

export function defaultSubwayGraph(): SubwayGraph {
    return {
        nodes: [],
        edges: [],
        routes: {},
    }
}