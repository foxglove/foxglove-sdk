# Foxglove Marketing Page Navigation Structure

## Current Navigation (as of Feb 2026)

### Top-level Items

| Nav Item | Type | Sub-items |
|----------|------|-----------|
| **Product** | Dropdown | Visualization, Data Management, Agent, MCAP, Integrations, Extensibility, Security, Download |
| **Solutions** | Dropdown | Automotive, Defense & Aerospace, Logistics & Supply Chain, Manufacturing, Agriculture & Forestry, Marine, Construction & Mining, Health & Wellbeing |
| **Resources** | Dropdown | Why Foxglove, Documentation, Changelog, Blog, API Reference, Tutorials, Status, Careers, Contact Us, Help and Support (Discord) |
| **Customers** | Direct link | — |
| **Pricing** | Direct link | — |
| *Right side:* | | Download, Examples, Sign in, Get started for free (CTA) |

---

## Recommended Navigation Structure

### Design Principles

1. **Audience-first**: Serve the three key audiences — individual robotics developers, engineering managers evaluating tools, and enterprise decision-makers
2. **Reduce cognitive load**: Each dropdown should have a clear, single purpose with no more than ~8 items
3. **Separate concerns**: Don't mix developer resources, company info, and marketing content in one dropdown
4. **Promote conversion paths**: Keep "Pricing" and the primary CTA ("Get started for free") always visible
5. **Surface high-value pages**: "Why Foxglove" and "Customers" are key decision-influencing pages and should be easy to find

---

### Proposed Top-Level Nav Items

```
[Logo]   Product   Solutions   Developers   Customers   Pricing     |  Sign in   [Get started for free]
```

There should be **5 top-level nav items**: Product, Solutions, Developers, Customers, and Pricing. "Sign in" and "Get started for free" remain as utility/CTA links on the right side.

---

### 1. Product (Dropdown — Mega Menu)

The Product dropdown showcases what Foxglove does. Group items into logical sections with clear labels.

**Core Platform**
| Item | Path | Description |
|------|------|-------------|
| Visualization | `/product/visualization` | Visualize multimodal robotics data in one integrated platform |
| Data Management | `/product/data-management` | Collect, organize, and search across all your robot data |
| Agent | `/product/agent` | Connect to robots and remote data sources in the field |

**Infrastructure & Ecosystem**
| Item | Path | Description |
|------|------|-------------|
| MCAP | `/product/mcap` | High-performance open-source logging format for multimodal data |
| Integrations | `/product/integrations` | Connect with ROS, PX4, custom frameworks, and more |
| Extensibility | `/product/extensibility` | Build custom panels, extensions, and workflows |

**Footer links in dropdown**
| Item | Path |
|------|------|
| Security | `/security` |
| Download Desktop App | `/download` |
| See all product features → | `/product` (overview page) |

**Rationale:**
- Grouping into "Core Platform" vs. "Infrastructure & Ecosystem" helps visitors quickly understand the product architecture
- Security moves out of being a peer to product features and becomes a supporting trust link
- Download and a product overview link are accessible but don't compete with feature pages

---

### 2. Solutions (Dropdown — Grid Layout)

The Solutions dropdown helps visitors find content relevant to their industry. Keep the current industry vertical structure — it's strong for SEO and helps enterprise buyers self-identify.

**By Industry**
| Item | Path |
|------|------|
| Automotive | `/solutions/automotive` |
| Defense & Aerospace | `/solutions/defense-and-aerospace` |
| Logistics & Supply Chain | `/solutions/logistics-and-supply-chain` |
| Manufacturing | `/solutions/manufacturing` |
| Agriculture & Forestry | `/solutions/agriculture-and-forestry` |
| Marine | `/solutions/marine` |
| Construction & Mining | `/solutions/construction-and-mining` |
| Health & Wellbeing | `/solutions/health-and-wellbeing` |

**Highlighted link**
| Item | Path |
|------|------|
| Why Foxglove? | `/why-foxglove` |

**Rationale:**
- "Why Foxglove?" moves here from Resources — it's fundamentally a solutions/positioning page that helps evaluators compare options
- A 2-column or 4×2 grid layout for industries works well visually
- Consider adding a "Featured customer story" card in the dropdown to reinforce social proof in-context

---

### 3. Developers (Dropdown)

Create a dedicated developer-focused dropdown. This was previously called "Resources" but mixed developer content with company content. A "Developers" label clearly signals this is the technical audience's entry point.

**Get Started**
| Item | Path | Description |
|------|------|-------------|
| Documentation | `https://docs.foxglove.dev/docs/introduction/` | Guides, concepts, and reference |
| API Reference | `https://docs.foxglove.dev/api` | REST API documentation |
| SDK | `https://docs.foxglove.dev/sdk` | Python, C++, Rust, and TypeScript SDKs |
| Examples | `/examples` | Sample layouts and visualizations |

**Learn**
| Item | Path | Description |
|------|------|-------------|
| Tutorials | `/blog?topic=tutorial` | Step-by-step guides |
| Blog | `/blog` | Engineering posts and product updates |
| Changelog | `https://docs.foxglove.dev/changelog/` | Latest releases and improvements |

**Community & Support**
| Item | Path | Description |
|------|------|-------------|
| Community (Discord) | `https://discord.com/invite/vUVAdFmMFM` | Join the Foxglove community |
| Status | `https://foxglovestatus.com/` | System status and uptime |

**Rationale:**
- Renaming from "Resources" to "Developers" makes the audience explicit — this is now a focused developer hub
- "Examples" moves from the right-side nav into this dropdown where developers expect to find it
- SDK is surfaced as a distinct item (currently not directly linked in nav) — it's a key developer touchpoint
- Removes Careers and Contact Us, which don't belong in a developer dropdown

---

### 4. Customers (Direct Link or Dropdown)

**Option A — Direct Link (recommended for now):**

Keep Customers as a direct link to `/customers` (the customer stories page). This is clean and simple.

**Option B — Small Dropdown (if content grows):**

If Foxglove accumulates more social-proof content, consider a small dropdown:

| Item | Path | Description |
|------|------|-------------|
| Customer Stories | `/customers` | See how teams use Foxglove |
| Case Studies | `/customer-stories` | In-depth success stories |
| Actuate (Conference) | `https://actuate.foxglove.dev` | Annual robotics event |

**Rationale:**
- Customer stories are critical for mid-funnel buyers and deserve top-level visibility
- Keeping it as a direct link avoids unnecessary clicks for this high-intent page

---

### 5. Pricing (Direct Link)

Keep Pricing as a direct link to `/pricing`. No dropdown needed.

**Rationale:**
- Pricing should always be one click away — it's one of the most-visited pages for any SaaS product
- No sub-navigation needed; the pricing page itself should handle plan comparison

---

### Right-Side Utility Nav

| Item | Path | Notes |
|------|------|-------|
| Sign in | `https://app.foxglove.dev/signin` | Text link, lower visual weight |
| **Get started for free** | `https://app.foxglove.dev/signup` | Primary CTA button (purple) |

**Changes from current:**
- Remove "Download" from right-side nav (it's now in the Product dropdown)
- Remove "Examples" from right-side nav (it's now in the Developers dropdown)
- This keeps the right side clean and conversion-focused: just sign in + signup CTA
- On mobile, add a "Get a demo" CTA (already exists) and collapse the nav into a hamburger menu

---

### Mobile Navigation

On mobile (< 992px), the navigation collapses to a hamburger menu with accordion-style dropdowns:

1. Product (expandable)
2. Solutions (expandable)
3. Developers (expandable)
4. Customers (direct link)
5. Pricing (direct link)
6. Sign in
7. **Get started for free** (sticky CTA at bottom)
8. **Get a demo** (secondary CTA)

---

## Company/About Pages — Where Do They Live?

Currently, "Careers" and "Contact Us" are buried in the Resources dropdown, and "About Us" only exists in the footer. These company pages don't warrant a top-level nav item but shouldn't be hard to find.

**Recommendation:** Keep company pages accessible via the **footer only**:

**Footer — Company Column:**
- About Us (`/about`)
- Careers (`/careers`)
- Media Kit (`/media`)
- Community (`/community`)
- Actuate (`https://actuate.foxglove.dev`)
- Contact Us (`/contact`)

This is standard practice for B2B SaaS sites — company info is expected in the footer, not the primary nav. The primary nav should focus on product, solutions, developer resources, social proof, and pricing.

---

## Summary of Changes from Current Nav

| Change | Why |
|--------|-----|
| Rename "Resources" → "Developers" | Clarifies the audience; removes ambiguity |
| Move "Why Foxglove?" from Resources to Solutions | It's a positioning/evaluation page, not a developer resource |
| Move "Careers" and "Contact Us" to footer only | Company info doesn't belong in a Resources/Developers dropdown |
| Move "Examples" from right-side nav to Developers dropdown | Consolidates developer tools in one place |
| Move "Download" from right-side nav to Product dropdown | Reduces right-side clutter; Download is a product action |
| Add "SDK" link in Developers dropdown | Key developer touchpoint currently missing from primary nav |
| Group Product items into "Core Platform" and "Infrastructure & Ecosystem" | Helps visitors understand the product architecture at a glance |
| Add "Why Foxglove?" link in Solutions dropdown | Helps evaluators find competitive positioning content |
| Clean up right-side nav to only Sign in + CTA | Maximizes conversion focus |

---

## Visual Wireframe

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ [Foxglove Logo]  Product ▾  Solutions ▾  Developers ▾  Customers  Pricing  │
│                                                    Sign in  [Get started]  │
└─────────────────────────────────────────────────────────────────────────────┘

Product Dropdown:                    Solutions Dropdown:
┌──────────────────────────────┐     ┌────────────────────────────────────┐
│ CORE PLATFORM                │     │ BY INDUSTRY                        │
│  ○ Visualization             │     │  ┌──────────────┬────────────────┐ │
│  ○ Data Management           │     │  │ Automotive    │ Agriculture &  │ │
│  ○ Agent                     │     │  │ Defense &     │   Forestry     │ │
│                              │     │  │  Aerospace    │ Marine         │ │
│ INFRASTRUCTURE & ECOSYSTEM   │     │  │ Logistics &   │ Construction & │ │
│  ○ MCAP                     │     │  │  Supply Chain │   Mining       │ │
│  ○ Integrations             │     │  │ Manufacturing │ Health &       │ │
│  ○ Extensibility            │     │  │               │   Wellbeing    │ │
│ ─────────────────────────── │     │  └──────────────┴────────────────┘ │
│  Security  ·  Download       │     │                                    │
│  See all features →          │     │  ★ Why Foxglove?                   │
└──────────────────────────────┘     └────────────────────────────────────┘

Developers Dropdown:
┌──────────────────────────────────────────────┐
│ GET STARTED           │ LEARN                │
│  ○ Documentation      │  ○ Tutorials         │
│  ○ API Reference      │  ○ Blog              │
│  ○ SDK                │  ○ Changelog          │
│  ○ Examples           │                      │
│                       │ COMMUNITY & SUPPORT  │
│                       │  ○ Discord Community  │
│                       │  ○ Status            │
└──────────────────────────────────────────────┘
```
