export interface Meeting {
  id: number;
  platform: string;
  native_meeting_id: string;
  title?: string;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface TranscriptSegment {
  id: string;
  meeting_id: number;
  speaker?: string;
  text: string;
  start_time: number;
  end_time: number;
  created_at: string;
}

export interface User {
  id: number;
  email: string;
  name?: string;
  created_at: string;
}

export interface ApiToken {
  id: number;
  token: string;
  name?: string;
  created_at: string;
  last_used_at?: string;
}
