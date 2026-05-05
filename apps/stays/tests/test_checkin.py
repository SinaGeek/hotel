from django.test import TestCase
from datetime import date
from apps.hotels.models import Hotel
from apps.rooms.models import RoomType, Room, RoomStatus
from apps.guests.models import Guest
from apps.reservations.models import Reservation
from apps.stays.services.stay_service import StayService
class CheckinTest(TestCase):
    def test_checkin_changes_room_status(self):
        h=Hotel.objects.create(name='H',code='h2'); rt=RoomType.objects.create(hotel_id=h.id,name='Std',base_rate=100)
        room=Room.objects.create(hotel_id=h.id,number='101',room_type=rt,status=RoomStatus.CLEAN)
        g=Guest.objects.create(hotel_id=h.id,first_name='A',last_name='B')
        res=Reservation.objects.create(hotel_id=h.id,guest=g,room=room,check_in_date=date(2026,1,1),check_out_date=date(2026,1,2),source='WALK_IN')
        StayService.check_in(res)
        room.refresh_from_db(); self.assertEqual(room.status,RoomStatus.OCCUPIED)
