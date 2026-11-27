# Anywhere Cloud

## Why

A user who wants to reach a second machine sets it up by hand today. The phone pairs with each Mac by QR code and needs Tailscale. The desktop adds each host by SSH. Nothing ties the machines together, so each new device repeats the work on every machine.

Anywhere Cloud gives a user one account. Every machine and every app signed in to the account finds the others, and a paying user reaches them from any network.

Paradev solves the same problem, and its design is the starting point. Its cloud holds the account, the list of hosts and a relay. Each host keeps its own projects and workspaces in a local database (`packages/host-service` in `paradevai/paradev`). Anywhere already has that shape: `docs/plans/2026-11-24-pocketd-owns-shared-state.md` makes pocketd the owner of shared state on each machine.

## Decisions

### What the cloud is

1. The cloud holds the account, the list of the account's members, and a relay. pocketd stays the owner of projects, settings, Terminals and sessions on its machine. The cloud never stores them.
2. The cloud is optional. Without an account, Anywhere works as it does today: pairing by QR code, hosts over SSH, the phone over the tailnet. Those paths stay.
3. Teams, cloud machines, settings that follow the user, and email sign-in are out of scope.

### Accounts

4. A user signs in with GitHub, Google or Apple. There are no passwords. The App Store requires Sign in with Apple beside the other two.
5. The desktop opens the browser to sign in and returns through an `anywhere://` link. On a machine with no screen, `pocketd login` prints a link and a short code, and the user opens the link on any device.

### Members

A **member** is one app or one pocketd that signed in to the account and was let in. Each member makes its own key pair and keeps the private key on its device. The cloud stores the public keys.

6. The first member of an account needs no approval. Every later member needs one existing member to approve it once. The rule is the same for an app and for a machine.
7. Every member that is online shows the request. One approval lets the new member reach every machine of the account. One refusal cancels the request everywhere. For a machine with no screen, the request shows the short code that `pocketd login` printed, and the user compares the two.
8. Any member can remove another member. On the phone, removing a member needs Face ID or the passcode. The cloud stops serving a removed member at once. A machine that was offline applies the removal when it next reaches the cloud.
9. A member's kind sets what it can do. A desktop member is the owner on a machine: it can run `exec` and `fs.*`. A phone member has the scopes of a paired phone, under `phone.maxAccess`.
10. The account has a recovery code, shown once when the first member joins. A new device that presents the code becomes a member with no approval. The cloud never receives the code.
11. A user with no member left and no code can reset the members. The new list holds only the device that reset it, and the cloud signs every other device out. A machine joins the new list only when the user runs `pocketd login --reset` on that machine, at its keyboard or over SSH.
12. There is no recovery by email. An email link would let anyone who controls the cloud or the mailbox add a device.

### Connections

13. Two members talk through one encrypted channel that each end authenticates with the other's public key. The channel carries the protocol pocketd speaks today, unchanged. The same channel runs over a direct TCP connection or through the relay.
14. A machine reports its addresses to the cloud: its tailnet address and its LAN addresses. The cloud gives them only to members of the same account.
15. A member tries the direct addresses first and uses the relay when none answers. It returns to a direct address when one answers again. An SSH host the user added by hand counts as a direct path.
16. A machine is known by the `hostId` in its own state, so it appears once however it is reached.
17. The relay forwards bytes it can't read. pocketd keeps one outbound connection to the relay, so the machine needs no open port.

### Free and paid

18. A signed-in user who doesn't pay gets the account, the members and direct connections. The user's data doesn't pass through the cloud.
19. A paying user also gets the relay. The price is one flat amount per account each month, with no limit on machines or devices.
20. The relay is paid from the first release. There is no free beta for it.
21. The user pays on the web through Polar, which sells as the merchant of record and pays out to Vietnam. The desktop and the website have an upgrade button. The iOS app has none and reads the account's plan.
22. The account has a `plan` column. The relay reads it when a connection opens. The relay records the bytes each account sends and doesn't limit them in the first release.

### Infrastructure

23. The cloud is Go on Railway: an `api` service, a `relay` service and Railway's Postgres. The two services deploy apart, because a deploy of the relay drops every open connection.
24. The code lives in this repository beside `packages/pushrelay`.
25. Start on Railway's Hobby plan. Move to Pro when the relay has paying users.
26. Railway doesn't promise how long a WebSocket lives. pocketd and the apps send pings and reconnect with backoff.

## Goals the design must prove

These are aims, not facts. Step 2 tests each one before any of it ships.

- A person who controls the cloud can't reach a user's machine and can't read what passes through the relay.
- The cloud never learns the recovery code.
- A reset gives its author no access to a machine that was in the old list.
- A removed member can't reach a machine that has heard of the removal.

One limit is known already. The cloud can't forge a removal, but it can hide one from a machine. Step 2 decides how long a machine trusts a member list it hasn't refreshed.

## What the relay costs

Measured on 2026-11-25 with `nettop`, on the `ssh -L` link between one desktop and one host with 3 projects, while nobody used the host:

| Measure | Host to desktop | Desktop to host |
|---|---|---|
| 36 minutes from connect | 37.4 MB | 10.4 MB |
| 30 seconds | 498 KB | 137 KB |
| 30 seconds | 484 KB | 146 KB |

The link carries about 21 KB each second, or 76 MB each hour. Railway charges 0.05 USD for each GB that leaves it, and the relay forwards both directions. One desktop linked to one host all day and all night costs about 2.72 USD each month.

`docs/plans/2026-11-25-host-link-traffic.md` plans how to cut this traffic. Not yet measured: the phone's traffic, the relay's memory and CPU, and how many hours a day a paying user spends on the relay.

## Steps

Each step ships on its own.

1. **SQLite for pocketd.** `docs/plans/2026-11-25-pocketd-sqlite.md`.
2. **Spike.** Build a relay, the encrypted channel and the member rules as code to throw away, and measure them. `docs/plans/2026-11-25-anywhere-cloud-spike.md`.
3. **Accounts.** The `api` service, sign-in on the desktop, pocketd and iOS, a key pair for each member, and the list of machines.
4. **Members.** Approval, removal, the recovery code, reset, and direct connections between members.
5. **Relay and payment.** The `relay` service, the choice between a direct address and the relay, Polar, and the `plan` column.

Steps 3 to 5 each get a plan once step 2 has its numbers.

## Before step 5

- Open a Polar account and finish the payout setup for Vietnam. Polar needs a bank account in Vietnam that receives VND.
- Read Railway's terms for commercial use on the Hobby plan.
- Set the price above the measured cost of one account on the relay plus Polar's fee.
