import React from 'react';
import { Scatter } from 'react-chartjs-2';
import { Chart as ChartJS, CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend } from 'chart.js';

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend);

// Define the types for Stations and Stringlines
export interface Station {
    name: string;
    y: number;
}

export interface StringlinePoint {
    x: number,
    y: number,
}

export interface Stringline {
    points: StringlinePoint[],
    color: string,
}

interface TransitStringlineDiagramProps {
    stations: Station[];
    stringlines: Record<number, Stringline[]>;
}

export const StringlineChart: React.FC<TransitStringlineDiagramProps> = ({ stations, stringlines }) => {
    console.log(stations);
    console.log(stringlines);
    const primaryStringlines = stringlines[Object.keys(stringlines)[0] as any];
    const secondaryStringlines = stringlines[Object.keys(stringlines)[1] as any];
    const primaryDatasets = primaryStringlines?.map(stringline => ({
            label: 'Stringline',
            data: stringline.points,
            borderColor: stringline.color,
            backgroundColor: stringline.color,
            showLine: true,
            fill: false,
            pointRadius: 0,
        }));
    const secondaryDatasets = secondaryStringlines?.map(stringline => ({
            label: 'Stringline',
            data: stringline.points,
            borderColor: stringline.color,
            backgroundColor: stringline.color,
            showLine: true,
            fill: false,
            pointRadius: 0,
        }));
    const datasets = [...(primaryDatasets || []), ...(secondaryDatasets || [])];
    const data = {
        datasets,
    };

    const options = {
        scales: {
            y: {
                type: 'linear',
                position: 'left',
                ticks: {
                    callback: function(this: any, value: string | number, index: number, ticks: any) {
                        const station = stations.find(station => station.y === value);
                        return '$' + (station ? station.name : value);
                    },
                },
            },
            x: {
                type: 'linear',
                position: 'bottom',
            },
        },
    } as const;

    return <Scatter data={data} options={options} />;
};
