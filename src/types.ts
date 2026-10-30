export type GameMode = 'single' | 'lan'
export type BidMode = 'score' | 'rob'
export type Lifecycle = 'ready' | 'running' | 'paused'
export type Stage = 'lobby' | 'bidding' | 'playing' | 'allPassed' | 'finished'
export type Suit = 'spade' | 'heart' | 'club' | 'diamond' | 'joker'
export interface Card { id: number; rank: number; suit: Suit }
export type ComboKind = 'single' | 'pair' | 'triple' | 'tripleSingle' | 'triplePair' | 'straight' | 'pairStraight' | 'tripleStraight' | 'airplaneSingles' | 'airplanePairs' | 'fourTwoSingles' | 'fourTwoPairs' | 'bomb' | 'rocket'
export interface Combo { kind: ComboKind; mainRank: number; length: number; chain: number; label: string; cards: number[] }
export interface Settings { playerName: string; bidMode: BidMode; baseScore: number; turnSeconds: number; sound: boolean; showHints: boolean }
export interface PlayerView { seat: number; name: string; connected: boolean; bot: boolean; ready: boolean; autoplay: boolean; count: number; role: 'unknown' | 'landlord' | 'farmer'; score: number }
export interface SeatAction { seat: number; kind: 'bid' | 'play' | 'pass'; text: string; cards: Card[]; combo: Combo | null }
export interface PublicEvent { id: number; text: string }
export interface ResultView { winner: number; landlordWon: boolean; spring: string | null; unitScore: number; deltas: number[]; multiplier: number; baseScore: number }
export interface NetworkView { status: 'offline' | 'hosting' | 'connecting' | 'connected' | 'disconnected'; role: 'host' | 'guest' | null; address: string; message: string }
export interface ActionSignal { id: number; kind: string; seat: number | null }
export interface Snapshot {
  revision: number; sessionId: number; roundId: number; turnToken: number; mode: GameMode
  lifecycle: Lifecycle; stage: Stage; localSeat: number; players: PlayerView[]; myHand: Card[]
  bottom: Card[]; bottomHidden: boolean; turn: number | null; landlord: number | null
  bidMode: BidMode; bidOptions: number[]; bidPrompt: string; highestBid: number
  previous: Combo | null; previousSeat: number | null; table: (SeatAction | null)[]
  multiplier: number; baseScore: number; remainingMs: number | null; result: ResultView | null
  settings: Settings; network: NetworkView; events: PublicEvent[]; action: ActionSignal
  canStart: boolean; canControl: boolean; canPass: boolean
}
export type GameAction =
  | { type: 'start' } | { type: 'pause' } | { type: 'resume' } | { type: 'nextRound' }
  | { type: 'bid'; value: number } | { type: 'play'; cards: number[] } | { type: 'pass' }
  | { type: 'ready'; value: boolean } | { type: 'autoplay'; value: boolean }
export interface Hint { revision: number; sessionId: number; roundId: number; turnToken: number; cards: number[]; pass: boolean; message: string }