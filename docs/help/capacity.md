# Capacity

**Capacity** shows how loaded each person is, week by week, as a heatmap: people down the side, weeks across.

## How the load is worked out

1. The [schedule](help:schedule) says which working days each open task occupies.
2. A task of *d* days assigned at *a*% to someone counts *d × a%* days in each week it touches, so the weekly numbers add up to the task's size.
3. That is divided by the person's capacity: their **weekly hours** divided by your **hours per working day** (a 40-hour week at 8 hours a day is 5 days).

So the percentage is *scheduled work ÷ capacity*. Above 100% is overloaded. Unestimated tasks count as one day each (so the picture is only as good as your estimates), and finished tasks don't count.

## Reading the heatmap

Each cell shows the percentage, coloured by band:

| Band | Load |
|---|---|
| Light | under 50% |
| Good | 50–85% |
| Full | 85–100% |
| Over | over 100% |
| Heavy | over 125% |

**Click a cell** to see the tasks behind it. Switch between **4, 8 and 12 weeks** and move **earlier or later** with the arrows. The row also notes people who have **many open tasks**.

## Too many tasks

Estimates can be missing, so there is a second signal: a person with **more open tasks than a limit** (10 by default, set under *Settings → Thresholds*) is flagged on this screen and on the [Overview](help:overview) whatever the estimates say.

## Setting it up

- Give each person their **weekly capacity** in hours on the [People](help:people-teams) screen (part-timers 20, and so on). New people get the default from *Settings → General*.
- **Assign** tasks to people, with an allocation percentage when someone gives only part of their time.
- Estimate your tasks.
- If your week isn't Monday to Friday, change the working days in [Settings](help:settings); a 4-day week has 4 days of capacity.

## What it doesn't do

- It doesn't count **meetings**: they are left out of the load, like the rest of the plan. (See [Meetings](help:meetings).)

It doesn't level the load for you or look at holidays. It tells you where the problem is; moving a due date, reassigning or adding a *blocks* link is up to you. Use [What if](help:what-if) to see the effect.
