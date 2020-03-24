#!/usr/bin/env python3

import torch
from torch.optim import Adam
import argparse

parser = argparse.ArgumentParser()
parser.add_argument(
    '--data',
    # default='./data-0324.csv',
    default='./0324.csv',
    help='data(csv)'
)
args = parser.parse_args()

with open(args.data) as f:
    rows = [
        line.split('\n')[0].split(',')
        for line in f
    ]

data_x = torch.tensor([[float(r) for r in row[:2]] for row in rows])
mean_x = torch.mean(data_x, dim=0)
data_x -= mean_x

data_y = torch.tensor([[float(r) for r in row[2:4]] for row in rows])
mean_y = torch.mean(data_y, dim=0)
data_y -= mean_y

model = torch.nn.Linear(2, 2, bias=False)
optimizer = Adam(model.parameters(), lr=0.0001)
criterion = torch.nn.MSELoss()

assert len(data_x) == len(data_y)
SPLIT_RATIO = 0.7
NUM_STEPS = 15000
n_train = int(len(data_x) * SPLIT_RATIO)

data = {
    'train': {
        'x': data_x[:n_train],
        'y': data_y[:n_train]
    },
    'valid': {
        'x': data_x[n_train:],
        'y': data_y[n_train:]
    },
}


for step in range(1, NUM_STEPS):
    loss = criterion(model(data['train']['x']), data['train']['y'])
    optimizer.zero_grad()
    loss.backward()
    optimizer.step()
    if step % 5 == 0:
        loss = criterion(model(data['valid']['x']), data['valid']['y'])
        print(f'Step: {step:05}, Loss: {loss.item():.5}')

linear_transform = list(model.parameters())[0].data.numpy()
translation = mean_y.data.numpy() - linear_transform @ mean_x.data.numpy()


print(linear_transform)
print(translation)

