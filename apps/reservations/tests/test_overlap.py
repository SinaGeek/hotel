from django.test import TestCase
from datetime import date
from apps.hotels.models import Hotel
from apps.rooms.models import RoomType, Room
from apps.guests.models import Guest
from apps.reservations.services.reservation_service import ReservationService
from django.core.exceptions import ValidationError
class OverlapTest(TestCase):
    def test_overlap_blocked(self):
        h=Hotel.objects.create(name='H',code='h')
        rt=RoomType.objects.create(hotel_id=h.id,name='Std',base_rate=100)
        r=Room.objects.create(hotel_id=h.id,number='101',room_type=rt)
        g=Guest.objects.create(hotel_id=h.id,first_name='A',last_name='B')
        ReservationService.create(hotel_id=h.id,guest=g,room=r,check_in_date=date(2026,1,1),check_out_date=date(2026,1,3),source='WALK_IN')
        with self.assertRaises(ValidationError):
            ReservationService.create(hotel_id=h.id,guest=g,room=r,check_in_date=date(2026,1,2),check_out_date=date(2026,1,4),source='WALK_IN')
